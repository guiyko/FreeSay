use serde::{Deserialize, Serialize};
use thiserror::Error;

pub struct AtvvUuids;

impl AtvvUuids {
    pub const SERVICE: &'static str = "AB5E0001-5A21-4F05-BC7D-AF01F617B664";
    pub const TRANSMIT: &'static str = "AB5E0002-5A21-4F05-BC7D-AF01F617B664";
    pub const AUDIO: &'static str = "AB5E0003-5A21-4F05-BC7D-AF01F617B664";
    pub const CONTROL: &'static str = "AB5E0004-5A21-4F05-BC7D-AF01F617B664";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtvvControlEvent {
    Capabilities(AtvvCapabilities),
    MicrophoneOpenRequested,
    StreamStarted {
        interaction: Option<u8>,
        codec: Option<u8>,
        session_id: u8,
    },
    StreamStopped,
    DecoderSync {
        predictor: i16,
        step_index: u8,
    },
    Unknown {
        opcode: u8,
    },
}

impl AtvvControlEvent {
    pub fn parse(bytes: &[u8]) -> Result<Self, AtvvError> {
        let opcode = *bytes.first().ok_or(AtvvError::PacketTooShort)?;
        match opcode {
            0x0B => Ok(Self::Capabilities(AtvvCapabilities::parse(bytes)?)),
            0x08 => Ok(Self::MicrophoneOpenRequested),
            0x04 => Ok(Self::StreamStarted {
                interaction: bytes.get(1).copied(),
                codec: bytes.get(2).copied(),
                session_id: bytes.get(3).copied().unwrap_or(0),
            }),
            0x00 => Ok(Self::StreamStopped),
            0x0A => {
                if bytes.len() < 7 {
                    return Err(AtvvError::PacketTooShort);
                }
                Ok(Self::DecoderSync {
                    predictor: i16::from_be_bytes([bytes[4], bytes[5]]),
                    step_index: bytes[6],
                })
            }
            _ => Ok(Self::Unknown { opcode }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtvvCommand {
    GetCapabilitiesV10,
    MicrophoneOpen { version: u16, codec: u8 },
    MicrophoneClose { version: u16, session_id: u8 },
    MicrophoneExtend { version: u16, session_id: u8 },
}

impl AtvvCommand {
    pub fn encode(&self) -> Option<Vec<u8>> {
        match *self {
            Self::GetCapabilitiesV10 => Some(vec![0x0A, 0x01, 0x00, 0x00, 0x03, 0x03]),
            Self::MicrophoneOpen { version, codec: _ } if version >= 0x0100 => {
                Some(vec![0x0C, 0x00])
            }
            Self::MicrophoneOpen { codec, .. } => Some(vec![0x0C, 0x00, codec]),
            Self::MicrophoneClose {
                version,
                session_id,
            } if version >= 0x0100 => Some(vec![0x0D, session_id]),
            Self::MicrophoneClose { .. } => Some(vec![0x0D]),
            Self::MicrophoneExtend {
                version,
                session_id,
            } if version >= 0x0100 => Some(vec![0x0E, session_id]),
            Self::MicrophoneExtend { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtvvCapabilities {
    pub version: u16,
    pub codecs: u8,
    pub interaction: u8,
    pub frame_size: usize,
    pub selected_codec: u8,
    pub sample_rate: u32,
}

impl AtvvCapabilities {
    pub fn parse(bytes: &[u8]) -> Result<Self, AtvvError> {
        if bytes.len() < 7 {
            return Err(AtvvError::PacketTooShort);
        }
        if bytes[0] != 0x0B {
            return Err(AtvvError::UnexpectedMessage(bytes[0]));
        }

        let version = u16::from_be_bytes([bytes[1], bytes[2]]);
        let (mut codecs, mut interaction) = if version >= 0x0100 {
            (bytes[3], bytes[4])
        } else {
            if bytes.len() < 9 {
                return Err(AtvvError::PacketTooShort);
            }
            (bytes[4], 0)
        };

        if version >= 0x0100 && codecs == 0 && bytes.len() >= 9 && bytes[4] & 0x03 != 0 {
            codecs = bytes[4];
            interaction = 0x03;
        }

        let selected_codec = if codecs & 0x02 != 0 { 0x02 } else { 0x01 };
        let sample_rate = if selected_codec == 0x02 {
            16_000
        } else {
            8_000
        };
        let advertised_frame_size = u16::from_be_bytes([bytes[5], bytes[6]]) as usize;

        Ok(Self {
            version,
            codecs,
            interaction,
            frame_size: if advertised_frame_size == 0 {
                120
            } else {
                advertised_frame_size
            },
            selected_codec,
            sample_rate,
        })
    }

    /// Google Voice over BLE 规范允许 8 kHz 与 16 kHz 两种 IMA ADPCM 编码。
    /// 8 kHz 设备由 `upsample_2x` 转成 16 kHz 后接入固定的 WASAPI 输出链路，
    /// 因此同样视为可用；其他采样率仍拒绝。
    pub fn supports_sayall_audio(&self) -> bool {
        matches!(self.sample_rate, 8_000 | 16_000)
    }

    pub fn needs_resample(&self) -> bool {
        self.sample_rate == 8_000
    }
}

/// 8 kHz -> 16 kHz 线性插值。
///
/// 输出长度 = (n - 1) * 2 + 2，末尾复制最后一个样本，保证跨帧边界连续；
/// 中间值用 i32 计算后再转回 i16，避免两个 i16 直接相加溢出。
pub fn upsample_2x(samples: &[i16]) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(samples.len() * 2);
    for window in samples.windows(2) {
        let a = i32::from(window[0]);
        let b = i32::from(window[1]);
        out.push(window[0]);
        out.push(((a + b) / 2) as i16);
    }
    let last = *samples.last().unwrap_or(&0);
    out.push(last);
    out.push(last);
    out
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AtvvError {
    #[error("ATVV packet is too short")]
    PacketTooShort,
    #[error("unexpected ATVV message 0x{0:02X}")]
    UnexpectedMessage(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_one_capabilities() {
        let capabilities = AtvvCapabilities::parse(&[0x0B, 0x01, 0x00, 0x02, 0x03, 0, 120])
            .expect("valid capabilities");
        assert_eq!(capabilities.version, 0x0100);
        assert_eq!(capabilities.selected_codec, 0x02);
        assert_eq!(capabilities.sample_rate, 16_000);
        assert_eq!(capabilities.frame_size, 120);
        assert!(capabilities.supports_sayall_audio());
    }

    #[test]
    fn accepts_legacy_codec_layout_advertised_as_version_one() {
        let capabilities = AtvvCapabilities::parse(&[0x0B, 0x01, 0x00, 0x00, 0x02, 0, 120, 0, 0])
            .expect("legacy layout");
        assert_eq!(capabilities.selected_codec, 0x02);
        assert_eq!(capabilities.interaction, 0x03);
    }

    #[test]
    fn rejects_malformed_capabilities() {
        assert_eq!(AtvvCapabilities::parse(&[]), Err(AtvvError::PacketTooShort));
        assert_eq!(
            AtvvCapabilities::parse(&[0, 1, 0, 2, 3, 0, 120]),
            Err(AtvvError::UnexpectedMessage(0))
        );
    }

    #[test]
    fn accepts_8k_remotes_but_flags_resampling() {
        let capabilities = AtvvCapabilities::parse(&[0x0B, 0x01, 0x00, 0x01, 0x03, 0, 120])
            .expect("8 kHz capabilities");
        assert_eq!(capabilities.sample_rate, 8_000);
        assert_eq!(capabilities.selected_codec, 0x01);
        assert!(capabilities.supports_sayall_audio());
        assert!(capabilities.needs_resample());
    }

    #[test]
    fn only_8k_and_16k_are_accepted() {
        let mut capabilities = AtvvCapabilities::parse(&[0x0B, 0x01, 0x00, 0x02, 0x03, 0, 120])
            .expect("16 kHz capabilities");
        assert!(capabilities.supports_sayall_audio());
        assert!(!capabilities.needs_resample());
        for rate in [0, 4_000, 11_025, 22_050, 44_100, 48_000] {
            capabilities.sample_rate = rate;
            assert!(!capabilities.supports_sayall_audio(), "rate={rate}");
        }
    }

    #[test]
    fn upsample_2x_is_deterministic_and_bounded() {
        assert!(upsample_2x(&[]).is_empty());
        assert_eq!(upsample_2x(&[100]), vec![100, 100]);
        assert_eq!(upsample_2x(&[0, 100]), vec![0, 50, 100, 100]);
        assert_eq!(upsample_2x(&[100, 0]), vec![100, 50, 0, 0]);

        // i16 极值：中间值用 i32 计算，不得溢出或触发 panic。
        let extreme = upsample_2x(&[i16::MIN, i16::MAX]);
        assert_eq!(extreme, vec![i16::MIN, 0, i16::MAX, i16::MAX]);

        for n in 1..=32usize {
            let input: Vec<i16> = (0..n).map(|i| i as i16).collect();
            let out = upsample_2x(&input);
            assert_eq!(out.len(), (n - 1) * 2 + 2, "n={n}");
            assert_eq!(out[0], input[0]);
            assert_eq!(*out.last().unwrap(), input[n - 1]);
        }
    }

    #[test]
    fn encodes_version_specific_microphone_commands() {
        assert_eq!(
            AtvvCommand::MicrophoneOpen {
                version: 0x0100,
                codec: 2
            }
            .encode(),
            Some(vec![0x0C, 0])
        );
        assert_eq!(
            AtvvCommand::MicrophoneOpen {
                version: 1,
                codec: 2
            }
            .encode(),
            Some(vec![0x0C, 0, 2])
        );
        assert_eq!(
            AtvvCommand::MicrophoneClose {
                version: 0x0100,
                session_id: 7
            }
            .encode(),
            Some(vec![0x0D, 7])
        );
        assert_eq!(
            AtvvCommand::MicrophoneExtend {
                version: 0x0100,
                session_id: 7
            }
            .encode(),
            Some(vec![0x0E, 7])
        );
        assert_eq!(
            AtvvCommand::MicrophoneExtend {
                version: 1,
                session_id: 7
            }
            .encode(),
            None
        );
    }

    #[test]
    fn parses_stream_and_decoder_control_events() {
        assert_eq!(
            AtvvControlEvent::parse(&[0x04, 0x03, 0x02, 0x07]).unwrap(),
            AtvvControlEvent::StreamStarted {
                interaction: Some(0x03),
                codec: Some(0x02),
                session_id: 0x07,
            }
        );
        assert_eq!(
            AtvvControlEvent::parse(&[0x0A, 0, 0, 0, 0xFF, 0x9C, 12]).unwrap(),
            AtvvControlEvent::DecoderSync {
                predictor: -100,
                step_index: 12,
            }
        );
        assert_eq!(
            AtvvControlEvent::parse(&[0x55]).unwrap(),
            AtvvControlEvent::Unknown { opcode: 0x55 }
        );
    }

    #[test]
    fn rejects_short_decoder_sync() {
        assert_eq!(
            AtvvControlEvent::parse(&[0x0A, 0, 0]),
            Err(AtvvError::PacketTooShort)
        );
    }
}
