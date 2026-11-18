// English strings for permissions. Key = exact Chinese source passed to t().
export default {
  // 页面标题 / 设置分组
  "权限": "Permissions",
  "诊断与日志": "Diagnostics & Logs",

  // 权限状态
  "语音链路已就绪": "Voice ready",
  "待验证": "Pending verification",
  "当前电脑不支持": "Not supported on this PC",
  "已运行": "Running",
  "已就绪": "Ready",
  "待选择设备": "Select a device",

  // 权限列表
  "蓝牙": "Bluetooth",
  "读取已配对的遥控器并建立连接。": "Read paired remotes and establish a connection.",
  "按键监听与映射": "Button Listening & Mapping",
  "只监听小米遥控器的按键；只有你为按键配置了动作时，才会向电脑发送按键。":
    "Only listens to Xiaomi remote buttons. Keys are sent to the PC only when you have assigned an action to a button.",
  "音频设备": "Audio Device",
  "语音设备由你明确选择，不改动系统默认设备。":
    "You explicitly choose the voice device. System default devices are not changed.",

  "检查并修复蓝牙加密记录": "Check and repair the Bluetooth encryption record",
  "检查中…": "Checking…",
  "正在检查蓝牙加密记录…": "Checking the Bluetooth encryption record…",
  "每次配对后，Windows 可能把这条记录的加密密钥长度写成 0，导致按键正常但语音用不了。点这里检查并修复。":
    "After every pairing, Windows may write the encryption key length of this record as 0, which leaves the buttons working but voice input silent. Click here to check and repair it.",
  "当前没有连接遥控器：请先在「连接」页连上一支再修复。":
    "No remote is connected. Connect one on the Connection page first, then repair.",
  "找不到当前连接的设备，请到「连接」页重新连接后再试。":
    "Couldn't find the connected device. Reconnect it on the Connection page and try again.",
  "检查失败，请稍后重试。": "The check failed. Please try again later.",
  "蓝牙加密记录正常，无需修复。": "The Bluetooth encryption record is healthy; nothing to repair.",
  "已修复蓝牙加密记录并重启蓝牙服务，请重新连接遥控器后测试语音。":
    "The Bluetooth encryption record was repaired and the Bluetooth service restarted. Reconnect the remote and test voice input.",
  "记录已修复，但蓝牙服务没能重启。请重启电脑后再试语音。":
    "The record was repaired, but the Bluetooth service could not be restarted. Restart your computer and try voice input again.",
  "这台设备的绑定已损坏（密钥为空），自动修复无法恢复，需要在系统设置里删除设备后重新配对。":
    "This device's pairing is broken (empty key) and cannot be repaired automatically. Remove the device in Windows Settings and pair it again.",
  "检测到记录异常，请再点一次按钮执行修复。": "An anomaly was detected. Click the button once more to apply the repair.",
  "没有权限修改该记录：请确认「全按键支持」已开启（助手需要运行）。":
    "No permission to modify the record: make sure \"Full button capture\" is enabled (the helper must be running).",
  "助手没有响应：请确认「全按键支持」已开启，稍后再试。":
    "The helper did not respond: make sure \"Full button capture\" is enabled and try again later.",
  "没有找到这台设备的绑定记录，可以到系统设置里重新配对一次。":
    "No binding record was found for this device. Try pairing it again in Windows Settings.",
  // 诊断摘要
  "诊断摘要": "Diagnostic Summary",
  "摘要不含设备地址、语音内容等隐私信息，可放心复制发给开发者排查问题。":
    "The summary contains no device addresses, voice content, or other private information. You can safely copy it to a developer for troubleshooting.",
  "生成中…": "Generating…",
  "生成摘要": "Generate Summary",
  "复制摘要": "Copy Summary",
  "正在打开…": "Opening…",
  "打开日志目录": "Open Log Folder",
  "日志记录本机运行细节，遇到问题时连同摘要一起发给开发者。":
    "Logs record local runtime details. If you run into a problem, send them to a developer together with the summary.",
  "尚未生成诊断摘要": "No diagnostic summary has been generated yet",
  "正在读取当前运行状态…": "Reading the current app status…",
  "诊断摘要已生成；复制前可在页面内检查全部内容":
    "Diagnostic summary generated. Review everything on this page before copying.",
  "当前环境不支持剪贴板写入。": "Clipboard writing is not supported in this environment.",
  "诊断摘要已复制到剪贴板": "Diagnostic summary copied to the clipboard.",
  "正在打开日志目录…": "Opening the log folder…",
  "已打开日志目录：{directory}": "Log folder opened: {directory}",

  // 失败文案（原始错误只进日志）
  "无法生成诊断摘要，请稍后重试。": "Couldn't generate the diagnostic summary. Please try again later.",
  "无法复制诊断摘要，请稍后重试。": "Couldn't copy the diagnostic summary. Please try again later.",
  "无法打开日志目录，请稍后重试。": "Couldn't open the log folder. Please try again later.",
} as Record<string, string>;
