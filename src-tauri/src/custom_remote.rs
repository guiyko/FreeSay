//! 自定义遥控器：用户自带遥控器的外观、按键命名、图片标点与按键绑定。
//!
//! 设计取舍（2026-11-12）：
//! - **单文件承载全部数据**：`<app_config_dir>/custom-remotes.json`。照片以 data URL
//!   内联存储——前端上传时会先缩到 ≤1024px 的 JPEG，单张通常 100–250 KB，几个遥控器
//!   完全可控；换来的是不需要自定义资源协议、不引入新的目录遍历与清理逻辑。
//! - **不猜测遥控器型号**：按键绑定记录的是用户实测按下的虚拟键码（`vk`），由现有
//!   `begin_key_observation` / `end_key_observation` 通道捕获，因此任何能进 Windows
//!   输入栈的遥控器都能绑，不依赖内置的四款白名单。
//! - **坐标用 0..1 相对值**：照片尺寸在界面上随窗口变化，写死像素会在不同分辨率下
//!   偏移；相对坐标由前端按图片自身宽高换算。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

/// 单个按键：名称、逻辑键位、核对的按键序列、照片上的相对坐标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRemoteButton {
    pub id: String,
    pub name: String,
    /// 逻辑键位（`power` / `up` / `voice` …）：决定动作落在哪个
    /// `mappings.actions` 槽位。`None`/`voice` 表示只展示。
    #[serde(default)]
    pub slot: Option<String>,
    /// 卡片放在照片哪一侧（`left`/`right`）；`None` = 按锚点横坐标自动判定。
    #[serde(default)]
    pub side: Option<String>,
    /// 用户按下遥控器时录到的按键序列，仅用于界面核对回显。
    ///
    /// 2026-11-12：字段名必须与前端一致（`keys`），否则 serde 会把它当未知字段丢掉，
    /// 于是"绑定成功、重开页面后绑定消失"。
    #[serde(default)]
    pub keys: Vec<String>,
    /// 旧版本的虚拟键码字段，保留只为读取历史文件。
    #[serde(default)]
    pub vk: Option<u32>,
    /// 旧版本的按键类型字段，保留只为读取历史文件。
    #[serde(default)]
    pub kind: Option<String>,
    /// 照片上的相对坐标（0..1），用于画锚点与箭头。
    pub x: f32,
    pub y: f32,
}

/// 一只自定义遥控器。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRemote {
    pub id: String,
    pub name: String,
    /// 照片 data URL（`data:image/jpeg;base64,...`）；`None` 表示还没上传。
    #[serde(default)]
    pub image: Option<String>,
    /// 照片像素尺寸：画布按它算等比容器，锚点才不会偏。
    #[serde(default, rename = "imageWidth")]
    pub image_width: Option<u32>,
    #[serde(default, rename = "imageHeight")]
    pub image_height: Option<u32>,
    #[serde(default)]
    pub buttons: Vec<CustomRemoteButton>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CustomRemoteStore {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    active_id: Option<String>,
    #[serde(default)]
    remotes: Vec<CustomRemote>,
}

fn store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("custom-remotes.json"))
        .map_err(|error| format!("无法定位配置目录：{error}"))
}

fn read_store(app: &tauri::AppHandle) -> CustomRemoteStore {
    let Ok(path) = store_path(app) else {
        return CustomRemoteStore::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return CustomRemoteStore::default();
    };
    match serde_json::from_str::<CustomRemoteStore>(&text) {
        Ok(mut store) => {
            if store.schema_version == 0 {
                store.schema_version = 1;
            }
            store
        }
        Err(error) => {
            // 解析失败不覆盖磁盘内容：保留用户数据，交由用户手工修复或重新保存。
            sayall_windows::gatt_note(format!(
                "custom_remote action=load phase=completed terminal_result=failed error={error}"
            ));
            CustomRemoteStore::default()
        }
    }
}

fn write_store(app: &tauri::AppHandle, store: &CustomRemoteStore) -> Result<(), String> {
    let path = store_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建配置目录：{error}"))?;
    }
    let text = serde_json::to_string_pretty(store)
        .map_err(|error| format!("序列化自定义遥控器失败：{error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("写入自定义遥控器失败：{error}"))
}

/// 读取全部自定义遥控器与当前生效项。
#[tauri::command]
pub fn get_custom_remotes(
    app: tauri::AppHandle,
) -> Result<(Vec<CustomRemote>, Option<String>), String> {
    let store = read_store(&app);
    Ok((store.remotes, store.active_id))
}

/// 整体保存（编辑器每次改动后调用，语义简单、无需增量合并）。
#[tauri::command]
pub fn save_custom_remotes(
    app: tauri::AppHandle,
    remotes: Vec<CustomRemote>,
    active_id: Option<String>,
) -> Result<(), String> {
    let mut store = read_store(&app);
    store.schema_version = 1;
    store.remotes = remotes;
    store.active_id = active_id;
    write_store(&app, &store)?;
    sayall_windows::gatt_note(format!(
        "custom_remote action=save phase=completed terminal_result=passed remotes={} active={}",
        store.remotes.len(),
        store.active_id.as_deref().unwrap_or("none")
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_round_trips_through_json() {
        let button = CustomRemoteButton {
            id: "power".to_owned(),
            name: "电源".to_owned(),
            slot: Some("power".to_owned()),
            side: Some("left".to_owned()),
            keys: vec!["left_control".to_owned(), "m".to_owned()],
            vk: Some(0x0D),
            kind: None,
            x: 0.32,
            y: 0.14,
        };
        let text = serde_json::to_string(&button).expect("serialize");
        let back: CustomRemoteButton = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back.name, "电源");
        assert_eq!(back.slot.as_deref(), Some("power"));
        assert_eq!(back.keys, vec!["left_control".to_owned(), "m".to_owned()]);
        assert_eq!(back.y, 0.14);
    }

    #[test]
    fn store_defaults_are_empty() {
        let store = CustomRemoteStore::default();
        assert!(store.remotes.is_empty());
        assert_eq!(store.active_id, None);
        assert_eq!(store.schema_version, 0);
    }

    #[test]
    fn missing_optional_fields_are_tolerated() {
        let json = r#"{"id":"a","name":"我的遥控器","buttons":[]}"#;
        let remote: CustomRemote = serde_json::from_str(json).expect("deserialize");
        assert_eq!(remote.image, None);
        assert_eq!(remote.image_width, None);
        assert!(remote.buttons.is_empty());
    }
}
