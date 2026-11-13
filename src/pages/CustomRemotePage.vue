<!--
  自定义遥控器（2026-11-12 用户指定）。

  为什么单独一页：内置四款遥控器是编译期白名单，用户手上的遥控器型号无穷多。
  这一页让用户自带外形与命名——上传照片、给按键起名、点一下照片标出位置、
  再选它的**逻辑键位**；「按键」页用同一套画布渲染（同一张照片、同款卡片与箭头）。
  单击/双击/长按的动作只在「按键」页编辑，两页共享同一份 `mappings.actions`。

  动作编辑**不再在本页重复实现**（2026-11-12 用户现场：本页只有 5 个选项，按键页有
  几十个）。本页只负责"这只遥控器长什么样、有哪些键、每个键是哪个逻辑键位"，
  动作统一在「按键」页设置——同一个编辑器，选项自然完全一致，也不会出现两套配置打架。

  交互约定：
  - 绑定复用现有快捷键录入通道（`start_shortcut_capture` + 边沿订阅），只作核对回显；
  - 标点是"先选条目、再点照片"两步，避免照片点击被两种语义抢；
  - 所有改动经 400ms 防抖后整体落盘，不需要用户记得按保存。
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import {
  createCustomButton,
  createCustomRemote,
  formatKeys,
  loadCustomRemotes,
  persistCustomRemotes,
  readRemotePhoto,
  type CustomRemote,
  type CustomRemoteButton,
} from "../lib/custom-remote";
import {
  startShortcutCapture,
  stopShortcutCapture,
  subscribeButtonEdges,
  subscribeShortcutCaptureEdges,
  type KeyCode,
  type RemoteButton,
} from "../lib/bridge";
/**
 * 逻辑键位：决定动作归属与卡片图标，必须与 Rust 侧 `RemoteButton` 一一对应。
 * `voice` 是展示用的语音键（不参与 `mappings.actions`）。
 */
const SLOT_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "（未指定）" },
  { value: "power", label: "电源" },
  { value: "up", label: "方向上" },
  { value: "down", label: "方向下" },
  { value: "left", label: "方向左" },
  { value: "right", label: "方向右" },
  { value: "ok", label: "确定" },
  { value: "back", label: "返回" },
  { value: "home", label: "主页" },
  { value: "menu", label: "菜单" },
  { value: "tv", label: "电视" },
  { value: "volume_up", label: "音量+" },
  { value: "volume_down", label: "音量−" },
  { value: "volume_mute", label: "静音" },
  { value: "voice", label: "语音" },
];

const remotes = ref<CustomRemote[]>([]);
const activeId = ref<string | null>(null);
const selectedId = ref<string | null>(null);
const statusText = ref("");
const bindingButtonId = ref<string | null>(null);
const capturedKeys = ref<KeyCode[]>([]);
const markingButtonId = ref<string | null>(null);
const photoInput = ref<HTMLInputElement | null>(null);

let unsubscribeEdges: (() => void) | null = null;
/** 报告层边沿的订阅：返回 / 音量± 这类键在 Windows 侧**没有键盘事件**，
 *  只走 `button-edge`（`[EDGE]` 那条通道），键盘钩子永远看不到。 */
let unsubscribeButtons: (() => void) | null = null;
let saveTimer: number | null = null;

/**
 * 一次按压结束后自动收尾的静默窗口。
 *
 * 2026-11-13 现场：点「按一下」→ 按遥控器键 → 界面毫无变化（结果要等用户再点一次
 * 按钮才回填）→ 用户以为"没检测到"，再点一次时捕获列表已被重置。遥控器一次按压
 * 会连发修饰键 + 主键（如 Ctrl、M 两条边沿），彼此间隔几十毫秒，因此以**最后一次
 * 边沿之后** 600ms 无新键作为"这一组录完了"，自动结束并回填。
 */
const BINDING_IDLE_MS = 600;
let bindingIdleTimer: number | null = null;

/**
 * 录入态的绝对上限。进入录入后一按键都没按（用户点了按钮就去干别的），静默
 * 计时器永远不会启动，而拦截是全局的——回来按键盘会被无声吞掉。这里兜一层硬超时。
 */
const BINDING_TIMEOUT_MS = 15_000;
let bindingTimeoutTimer: number | null = null;

function clearBindingIdleTimer(): void {
  if (bindingIdleTimer !== null) {
    window.clearTimeout(bindingIdleTimer);
    bindingIdleTimer = null;
  }
  if (bindingTimeoutTimer !== null) {
    window.clearTimeout(bindingTimeoutTimer);
    bindingTimeoutTimer = null;
  }
}

function scheduleBindingIdleFinish(): void {
  clearBindingIdleTimer();
  bindingIdleTimer = window.setTimeout(() => {
    bindingIdleTimer = null;
    void finishBinding();
  }, BINDING_IDLE_MS);
}

/**
 * 录入态必须在窗口失焦时立即结束。
 *
 * 拦截通道是**全系统**生效的低级键盘钩子；用户切到别的程序后若还留着录入态，
 * 那边所有按键都会先被本应用裁决（现场："切到其他程序也没办法输入"）。所以失焦
 * 即收尾，宁可这一组没录完，也不能让别人的键盘停摆。
 */
function handleWindowBlur(): void {
  if (bindingButtonId.value) void finishBinding();
}

const selected = computed<CustomRemote | null>(
  () => remotes.value.find((remote) => remote.id === selectedId.value) ?? null,
);

const markingButton = computed<CustomRemoteButton | null>(() => {
  const remote = selected.value;
  if (!remote || !markingButtonId.value) return null;
  return remote.buttons.find((button) => button.id === markingButtonId.value) ?? null;
});

onMounted(async () => {
  const snapshot = await loadCustomRemotes();
  remotes.value = snapshot.remotes;
  activeId.value = snapshot.activeId;
  selectedId.value = snapshot.activeId ?? snapshot.remotes[0]?.id ?? null;
  window.addEventListener("blur", handleWindowBlur);
});

onBeforeUnmount(() => {
  unsubscribeEdges?.();
  unsubscribeButtons?.();
  clearBindingIdleTimer();
  window.removeEventListener("blur", handleWindowBlur);
  void stopShortcutCapture().catch(() => undefined);
  if (saveTimer !== null) window.clearTimeout(saveTimer);
});

/** 任何改动都走这里：防抖后整体落盘，避免拖动坐标时写爆磁盘。 */
function scheduleSave(): void {
  if (saveTimer !== null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    saveTimer = null;
    try {
      await persistCustomRemotes(remotes.value, activeId.value);
      statusText.value = "已保存";
    } catch (error) {
      statusText.value = `保存失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }, 400);
}

watch(remotes, scheduleSave, { deep: true });
watch(activeId, scheduleSave);

function addRemote(): void {
  const remote = createCustomRemote(`遥控器 ${remotes.value.length + 1}`);
  remotes.value = [...remotes.value, remote];
  selectedId.value = remote.id;
  statusText.value = "已新建，请上传照片并给按键起名";
}

function removeRemote(remote: CustomRemote): void {
  remotes.value = remotes.value.filter((item) => item.id !== remote.id);
  if (activeId.value === remote.id) activeId.value = null;
  if (selectedId.value === remote.id) selectedId.value = remotes.value[0]?.id ?? null;
}

function useRemote(remote: CustomRemote): void {
  activeId.value = remote.id;
  statusText.value = `「${remote.name}」已设为当前遥控器，「按键」页将按它的外形展示`;
}

async function pickPhoto(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  const remote = selected.value;
  input.value = "";
  if (!file || !remote) return;
  try {
    const photo = await readRemotePhoto(file);
    remote.image = photo.dataUrl;
    remote.imageWidth = photo.width;
    remote.imageHeight = photo.height;
    statusText.value = `照片已载入（${photo.width}×${photo.height}），现在可以标点`;
  } catch (error) {
    statusText.value = `照片读取失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** 选中键位时顺手把空名称填上，减少重复输入。 */
function setSlot(button: CustomRemoteButton, value: string): void {
  button.slot = value === "" ? null : value;
  if (!button.name.trim()) {
    const option = SLOT_OPTIONS.find((item) => item.value === value);
    if (option && option.value) button.name = option.label;
  }
}

function addButton(): void {
  const remote = selected.value;
  if (!remote) return;
  remote.buttons = [...remote.buttons, createCustomButton("")];
  statusText.value = "已加一个按键：填名称、选键位，再点「标点」在照片上定位";
}

function removeButton(button: CustomRemoteButton): void {
  const remote = selected.value;
  if (!remote) return;
  remote.buttons = remote.buttons.filter((item) => item.id !== button.id);
  if (markingButtonId.value === button.id) markingButtonId.value = null;
}

/** 进入绑定态：录到的第一个按下沿序列即为该按键（只作核对回显）。 */
async function beginBinding(button: CustomRemoteButton): Promise<void> {
  if (bindingButtonId.value === button.id) {
    await finishBinding();
    return;
  }
  // 换绑到别的键：先把上一组收尾，避免两个按钮同时挂着一个录入订阅。
  if (bindingButtonId.value) await finishBinding();
  bindingButtonId.value = button.id;
  capturedKeys.value = [];
  markingButtonId.value = null;
  try {
    await startShortcutCapture();
    unsubscribeEdges = await subscribeShortcutCaptureEdges((edge) => {
      if (!edge.isPressed) return;
      // 2026-11-13：**注入边沿必须收**。RC003 走报告层时，遥控器按键是以
      // `LLKHF_INJECTED` 合成进系统的（`synth=0x01` 路径），后端已经在录入期
      // 明确接受这类副本（见 `key_gate::injected_edge_is_passthrough`），前端
      // 若再按 `source === "injected"` 丢弃，用户按遥控器就会"完全没反应"。
      // 自家映射注入不会出现在这里：核对期间没有任何语音会话在跑。
      if (capturedKeys.value.includes(edge.key)) return;
      capturedKeys.value = [...capturedKeys.value, edge.key];
      statusText.value = `已捕获 ${formatKeys(capturedKeys.value)}，继续按或稍候自动完成`;
      // 每条边沿都重置静默计时：遥控器一次按压会连发修饰键 + 主键。
      scheduleBindingIdleFinish();
    });
    // 报告层边沿（2026-11-13 现场）：返回 / 音量± / 主页 / 菜单这些键
    // **在 Windows 侧不产生任何键盘事件**（kbdhid 没有对应 VK 映射，直接被丢弃），
    // 所以上面的键盘钩子永远看不到它们 —— 用户点「核对」按这三个键会一直显示
    // "请按一下这个遥控器上的该键"，然后超时退出，看起来就是"捕捉不到"。
    // 它们只存在于 `button-edge`（助手在 HID 报告层识别出的 usage）。
    unsubscribeButtons = await subscribeButtonEdges((edge) => {
      if (!edge.isPressed) return;
      // 同一次按压可能两条通道都报（例如电源键在 OS 侧有键位映射）。
      // 以报告层为准直接替换，避免录成 "电源 + 回车" 这种双份。
      const key = edge.button as KeyCode;
      capturedKeys.value = [key];
      statusText.value = `已捕获 ${formatKeys(capturedKeys.value)}，松手稍候自动完成`;
      scheduleBindingIdleFinish();
    });
    statusText.value = "请按一下这个遥控器上的该键";
    // 硬超时：一按键都没按也要收尾，否则拦截会一直挂着。
    clearBindingIdleTimer();
    bindingTimeoutTimer = window.setTimeout(() => {
      bindingTimeoutTimer = null;
      void finishBinding();
    }, BINDING_TIMEOUT_MS);
  } catch (error) {
    bindingButtonId.value = null;
    clearBindingIdleTimer();
    statusText.value = `无法开始录入：${error instanceof Error ? error.message : String(error)}`;
  }
}

async function finishBinding(): Promise<void> {
  // 幂等：idle 计时器、按钮二次点击、窗口失焦三条路径都可能触发收尾。
  if (!bindingButtonId.value) return;
  const remote = selected.value;
  const target = remote?.buttons.find((button) => button.id === bindingButtonId.value);
  clearBindingIdleTimer();
  unsubscribeEdges?.();
  unsubscribeEdges = null;
  unsubscribeButtons?.();
  unsubscribeButtons = null;
  bindingButtonId.value = null;
  await stopShortcutCapture().catch(() => undefined);
  if (!target) return;
  const keys = capturedKeys.value;
  if (keys.length === 0) {
    statusText.value = "没有捕获到按键，请重试";
    return;
  }
  target.keys = keys;
  statusText.value = `「${target.name || "未命名按键"}」已绑定：${formatKeys(keys)}`;
}

/** 进入标点态：下一次点击照片即写入该条目的相对坐标。 */
function beginMarking(button: CustomRemoteButton): void {
  markingButtonId.value = markingButtonId.value === button.id ? null : button.id;
  statusText.value = markingButtonId.value ? "请在照片上点一下这个按键的位置" : "";
}

function markAt(event: MouseEvent): void {
  const button = markingButton.value;
  if (!button) return;
  const target = event.currentTarget as HTMLElement;
  const rect = target.getBoundingClientRect();
  button.x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
  button.y = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
  // `side` 自 2026-11-12 起只作历史字段保留：按键页改成「按添加顺序先排满左列、
  // 再排右列」（见 `lib/mapping-columns`），卡片在照片哪一侧不再由这个横坐标决定，
  // 照片上的橙点仍标在真实位置。这里照旧写入，是为了旧数据格式与回滚都能读。
  button.side = button.x < 0.5 ? "left" : "right";
  statusText.value = `坐标已记录：${(button.x * 100).toFixed(1)}% / ${(button.y * 100).toFixed(1)}%`;
  markingButtonId.value = null;
}

</script>

<template>
  <section class="custom-remote-page">
    <header class="page-header">
      <div>
        <h1>自定义遥控器</h1>
        <p class="subtitle">
          上传你在用的遥控器照片，给每个按键起名、选它的键位，再在照片上标出位置。
          绑定后的按键会和内置遥控器一样出现在「按键」页，动作也都在那一页设置。
        </p>
      </div>
      <button type="button" class="primary" @click="addRemote">新建遥控器</button>
    </header>

    <div class="layout">
      <aside class="list">
        <article
          v-for="remote in remotes"
          :key="remote.id"
          class="list-item"
          :class="{ selected: remote.id === selectedId, active: remote.id === activeId }"
          @click="selectedId = remote.id"
        >
          <div class="thumb">
            <img v-if="remote.image" :src="remote.image" :alt="remote.name" draggable="false" />
            <span v-else class="thumb-empty">无照片</span>
          </div>
          <div class="list-meta">
            <strong :title="remote.name || '未命名'">{{ remote.name || "未命名" }}</strong>
            <span>{{ remote.buttons.length }} 个按键</span>
          </div>
          <div class="list-actions">
            <button
              v-if="remote.id !== activeId"
              type="button"
              class="icon-button"
              title="设为当前遥控器（「按键」页按它的外形展示）"
              aria-label="设为当前遥控器"
              @click.stop="useRemote(remote)"
            >
              <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <circle cx="12" cy="12" r="7.2" stroke="currentColor" stroke-width="1.8" />
                <circle cx="12" cy="12" r="2.6" stroke="currentColor" stroke-width="1.8" />
                <path d="M12 2.6v3.2M12 18.2v3.2M2.6 12h3.2M18.2 12h3.2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
              </svg>
            </button>
            <span v-else class="badge">当前</span>
            <button
              type="button"
              class="icon-button danger"
              title="删除这只遥控器"
              aria-label="删除这只遥控器"
              @click.stop="removeRemote(remote)"
            >
              <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M4.5 7h15M9.5 7V4.8h5V7M6.8 7l.8 12.2h8.8L17.2 7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                <path d="M10.4 10.6v5.6M13.6 10.6v5.6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
              </svg>
            </button>
          </div>
        </article>
        <p v-if="remotes.length === 0" class="empty">还没有自定义遥控器，点右上角「新建遥控器」开始。</p>
      </aside>

      <main v-if="selected" class="editor">
        <div class="row">
          <label class="field">
            <span>遥控器名称</span>
            <input v-model="selected.name" type="text" placeholder="例如：客厅电视遥控器" />
          </label>
          <button type="button" class="ghost" @click="photoInput?.click()">
            {{ selected.image ? "更换照片" : "上传照片" }}
          </button>
          <input ref="photoInput" type="file" accept="image/*" hidden @change="pickPhoto" />
        </div>

        <div class="photo-area">
          <div v-if="selected.image" class="photo-wrap" :class="{ marking: !!markingButton }" @click="markAt">
            <img :src="selected.image" :alt="selected.name" draggable="false" />
            <span
              v-for="button in selected.buttons"
              :key="button.id"
              class="dot"
              :class="{ armed: markingButtonId === button.id }"
              :style="{ left: `${button.x * 100}%`, top: `${button.y * 100}%` }"
              :title="button.name"
            />
          </div>
          <p v-else class="photo-placeholder">先上传一张遥控器照片，然后就能在照片上点选按键位置。</p>
        </div>

        <div class="buttons-head">
          <h2>按键</h2>
          <button type="button" class="primary small" @click="addButton">添加按键</button>
        </div>

        <div class="table-wrap">
          <table class="button-table">
            <colgroup>
              <col style="width: 30%" />
              <col style="width: 18%" />
              <col style="width: 24%" />
              <col style="width: 12%" />
              <col style="width: 44px" />
            </colgroup>
            <thead>
              <tr>
                <th>名称</th>
                <th>键位</th>
                <th>核对（可选）</th>
                <th>位置</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="button in selected.buttons" :key="button.id">
                <td><input v-model="button.name" type="text" placeholder="电源 / 上 / 语音…" /></td>
                <td>
                  <select :value="button.slot ?? ''" @change="setSlot(button, ($event.target as HTMLSelectElement).value)">
                    <option v-for="option in SLOT_OPTIONS" :key="option.value" :value="option.value">
                      {{ option.label }}
                    </option>
                  </select>
                </td>
                <td>
                  <button type="button" class="ghost" :class="{ recording: bindingButtonId === button.id }" @click="beginBinding(button)">
                    {{ bindingButtonId === button.id ? "按一下…" : formatKeys(button.keys) || "按一下" }}
                  </button>
                </td>
                <td>
                  <button type="button" class="ghost" :class="{ recording: markingButtonId === button.id }" @click="beginMarking(button)">
                    {{ markingButtonId === button.id ? "点照片…" : "标点" }}
                  </button>
                </td>
                <!-- 2026-11-13 用户要求删除：单击/双击/长按三列在本页只是「按键」页的
                     只读镜像，点了还要跳页，占地方又没有可操作性。动作统一在「按键」页
                     设置，本页只负责外形、命名与键位归属。 -->
                <td>
                  <button
                    type="button"
                    class="icon-button danger"
                    title="删除这个按键"
                    aria-label="删除这个按键"
                    @click="removeButton(button)"
                  >
                    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
                      <path d="M4.5 7h15M9.5 7V4.8h5V7M6.8 7l.8 12.2h8.8L17.2 7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                    </svg>
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-if="selected.buttons.length === 0" class="empty">还没有按键，点「添加按键」逐个加。</p>
        <p class="hint">
          这里只管「这只遥控器长什么样、有哪几个键、每个键是哪个逻辑键位」；
          单击 / 双击 / 长按的动作统一在「按键」页设置，两页共用一份配置。
        </p>
      </main>

      <main v-else class="editor empty-editor">左侧新建或选中一只遥控器后在这里编辑。</main>
    </div>

    <footer v-if="statusText" class="status">{{ statusText }}</footer>
  </section>
</template>

<style scoped>
.custom-remote-page {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 24px 28px 32px;
  color: var(--text-primary);
  overflow-y: auto;
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.page-header h1 {
  margin: 0 0 6px;
  font-size: 22px;
  font-weight: 650;
}

.subtitle {
  margin: 0;
  max-width: 640px;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.6;
}

.layout {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  gap: 18px;
  align-items: start;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/**
 * 列表条目：一行三列（缩略图 / 名称+按键数 / 操作），全部 flex + align-items:center。
 *
 * 2026-11-12 用户现场："左侧图标、右侧文字没对齐"——旧版用 grid 双行区域
 * （`"thumb meta" "thumb actions"`），缩略图固定 44px 却跨两行，行高由右侧
 * 文字+按钮撑到 66px 以上，图标就顶在上沿、文字沉在下面，看起来永远错位。
 * 改成单行 flex 后，三列共用一条中线对齐，高度只由最高的那一格决定。
 */
.list-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 8px 8px 10px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--card);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease;
}

.list-item:hover {
  border-color: var(--border-strong);
}

.list-item.selected {
  border-color: var(--accent-border);
  background: var(--accent-surface);
}

.list-item.active {
  box-shadow: inset 0 0 0 1px var(--accent-border);
}

.thumb {
  flex: 0 0 auto;
  width: 40px;
  height: 40px;
  border-radius: 9px;
  overflow: hidden;
  background: var(--surface-subtle);
  display: grid;
  place-items: center;
}

/* contain 而不是 cover：遥控器照片是竖长的，cover 在 40px 方块里只剩中间一截。 */
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.thumb-empty {
  font-size: 9.5px;
  letter-spacing: 0.2px;
  color: var(--text-secondary);
}

.list-meta {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.list-meta strong {
  font-size: 13px;
  line-height: 1.35;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.list-meta span {
  font-size: 11px;
  line-height: 1.35;
  color: var(--text-secondary);
}

.list-actions {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 4px;
}

/* 操作按钮收成图标：260px 的列表列里放不下两个中文按钮，图标能保证整行不换行。 */
.icon-button {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
}

.icon-button svg {
  width: 14px;
  height: 14px;
}

.icon-button:hover {
  background: var(--surface-subtle);
  border-color: var(--border);
  color: var(--text-primary);
}

.icon-button.danger:hover {
  color: #b4322d;
}

.badge {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 7px;
  border-radius: 999px;
  background: var(--accent-surface-strong);
  color: var(--accent-text);
  font-size: 10.5px;
  font-weight: 600;
}

.editor {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--card);
}

.empty-editor {
  align-items: center;
  justify-content: center;
  min-height: 200px;
  color: var(--text-secondary);
}

.row {
  display: flex;
  align-items: flex-end;
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  font-size: 12px;
  color: var(--text-secondary);
}

.field input,
.button-table input,
.button-table select {
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: var(--surface-control);
  color: var(--text-primary);
  font-size: 13px;
}

.photo-area {
  border: 1px dashed var(--border-strong);
  border-radius: 12px;
  background: var(--surface-subtle);
  padding: 12px;
  display: grid;
  place-items: center;
  min-height: 220px;
}

.photo-wrap {
  position: relative;
  max-width: 420px;
  max-height: 420px;
}

.photo-wrap.marking {
  cursor: crosshair;
  outline: 2px solid var(--accent-border);
  outline-offset: 4px;
  border-radius: 8px;
}

.photo-wrap img {
  display: block;
  max-width: 100%;
  max-height: 420px;
  border-radius: 8px;
}

.dot {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  background: var(--accent);
  border: 2px solid #fff;
  box-shadow: 0 0 0 1px var(--accent-border);
}

.dot.armed {
  background: #e8730c;
}

.photo-placeholder {
  margin: 0;
  color: var(--text-secondary);
  font-size: 13px;
}

.buttons-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.buttons-head h2 {
  margin: 0;
  font-size: 15px;
}

.table-wrap {
  overflow-x: auto;
}

/* 8 列在窄窗口里会被挤成两行文字：给一个最小宽度，不够就横向滚动，不折行。 */
.button-table {
  width: 100%;
  min-width: 680px;
  border-collapse: separate;
  border-spacing: 0;
  font-size: 12px;
}

.button-table th {
  text-align: left;
  font-weight: 600;
  font-size: 11.5px;
  color: var(--text-secondary);
  padding: 8px 6px;
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.button-table td {
  padding: 6px;
  border-bottom: 1px solid var(--border);
  vertical-align: middle;
}

.button-table tbody tr:last-child td {
  border-bottom: 0;
}

.button-table tbody tr:hover td {
  background: var(--surface-subtle);
}

.button-table input {
  width: 100%;
}

.primary {
  height: 34px;
  padding: 0 14px;
  border: 0;
  border-radius: 9px;
  background: var(--accent);
  color: var(--text-on-accent);
  font-size: 13px;
  cursor: pointer;
}

.primary.small {
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
}

.ghost {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: var(--surface-control);
  color: var(--text-control);
  font-size: 12px;
  cursor: pointer;
}

.ghost.recording {
  border-color: var(--accent-border);
  background: var(--accent-surface);
  color: var(--accent-text);
}

.ghost.danger {
  color: #b4322d;
}

.empty {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
}

.hint {
  margin: 0;
  color: var(--text-secondary);
  font-size: 11px;
  line-height: 1.6;
}

.status {
  position: sticky;
  bottom: 0;
  padding: 8px 12px;
  border-radius: 10px;
  background: var(--accent-surface);
  color: var(--accent-text);
  font-size: 12px;
}
</style>
