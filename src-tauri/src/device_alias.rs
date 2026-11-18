//! 设备别名：给已配对的蓝牙遥控器起一个用户看得懂的名字。
//!
//! 为什么需要它（2026-11-14 现场）：同型号遥控器在系统里的名字完全相同
//! （两支都是「小米蓝牙语音遥控器」），连接页列出来的两行长得一模一样，
//! 用户根本不知道该点哪一行。设备身份能区分的只有蓝牙地址/令牌，而那是
//! 一串十六进制，不适合直接给人看。
//!
//! 存储键用 `device_token`（FNV-1a，与 `PairedRemote.token` 同一套算法），
//! **不用设备名**：名字会重复，令牌不会。别名只是显示层，不参与任何匹配逻辑。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Default, Serialize, Deserialize)]
struct AliasStore {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    aliases: BTreeMap<String, String>,
}

fn store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("device-aliases.json"))
        .map_err(|error| format!("无法定位配置目录：{error}"))
}

fn read_store(app: &tauri::AppHandle) -> AliasStore {
    let Ok(path) = store_path(app) else {
        return AliasStore::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AliasStore::default();
    };
    match serde_json::from_str::<AliasStore>(&text) {
        Ok(mut store) => {
            if store.schema_version == 0 {
                store.schema_version = 1;
            }
            store
        }
        Err(error) => {
            // 解析失败不覆盖磁盘：别名是用户手工输入的内容，宁可不用也不能丢。
            sayall_windows::gatt_note(format!(
                "device_alias action=load phase=completed terminal_result=failed error={error}"
            ));
            AliasStore::default()
        }
    }
}

fn write_store(app: &tauri::AppHandle, store: &AliasStore) -> Result<(), String> {
    let path = store_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建配置目录：{error}"))?;
    }
    let text =
        serde_json::to_string_pretty(store).map_err(|error| format!("序列化失败：{error}"))?;
    std::fs::write(&path, format!("{text}\n")).map_err(|error| format!("写入失败：{error}"))
}

/// 读取全部别名（键 = 设备令牌）。
#[tauri::command]
pub fn load_device_aliases(app: tauri::AppHandle) -> Result<BTreeMap<String, String>, String> {
    Ok(read_store(&app).aliases)
}

/// 设置/清除某个设备的别名；空字符串等价于清除。返回更新后的全量别名。
#[tauri::command]
pub fn save_device_alias(
    app: tauri::AppHandle,
    token: String,
    name: Option<String>,
) -> Result<BTreeMap<String, String>, String> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err("设备令牌不能为空".to_owned());
    }
    let mut store = read_store(&app);
    store.schema_version = 1;
    match name.map(|value| value.trim().to_owned()) {
        Some(value) if !value.is_empty() => {
            // 防止有人把别名写成一整段说明；超出就截断，避免把界面撑坏。
            let clipped: String = value.chars().take(24).collect();
            store.aliases.insert(token, clipped);
        }
        _ => {
            store.aliases.remove(&token);
        }
    }
    write_store(&app, &store)?;
    sayall_windows::gatt_note(
        "device_alias action=save phase=completed terminal_result=passed".to_owned(),
    );
    Ok(store.aliases)
}
