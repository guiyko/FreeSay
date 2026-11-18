<script setup lang="ts">
import { computed } from "vue";

import catalog from "../assets/recommendations-catalog.json";
import { t } from "../lib/i18n";
import { useLanguage } from "../lib/language";

/**
 * 推荐页（2026-11-13 改为离线固定内容）。
 *
 * 原来是「子 WebView 加载作者站点」：内容、筛选、详情全部来自远端页面，
 * 站点一改地址或网络受限，这里就只剩「正在加载…」。现在改为**随应用打包的
 * 静态内容**——不发任何网络请求，断网也能看。
 *
 * 数据来自 `src/assets/recommendations-catalog.json`（由原站脚本内嵌的
 * catalog 抽取：2 分类 / 2 分组 / 7 条目），图片已下载到
 * `public/recommendations/`，原站的来源追踪参数已剔除。
 */

type LocText = Record<string, string>;

interface CatalogAction {
  url: string;
  label: LocText;
}

interface CatalogItem {
  id: string;
  kind: string;
  title: LocText;
  summary?: LocText;
  reason?: LocText;
  compatibility?: LocText;
  image?: { url: string; alt?: LocText };
  actions?: CatalogAction[];
}

interface CatalogGroup {
  id: string;
  title: LocText;
  itemIds: string[];
}

const { effectiveLanguage } = useLanguage();

/** 内容语言跟界面语言走，只区分中英。 */
const locale = computed(() => (effectiveLanguage.value === "en" ? "en" : "zh-Hans"));

function localize(value?: LocText): string {
  if (!value) return "";
  return value[locale.value] ?? value["zh-Hans"] ?? "";
}

/** `reason` 是多行文本，拆成逐条要点更好读。 */
function reasonLines(value?: LocText): string[] {
  return localize(value)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

/** 同一条目可能带多条动作（按平台区分），取第一条有地址的。 */
function primaryAction(item: CatalogItem): CatalogAction | undefined {
  return item.actions?.find((action) => Boolean(action.url));
}

const groups = computed(() =>
  (catalog.groups as CatalogGroup[]).map((group) => ({
    id: group.id,
    title: localize(group.title),
    items: group.itemIds
      .map((id) => (catalog.items as CatalogItem[]).find((item) => item.id === id))
      .filter((item): item is CatalogItem => Boolean(item)),
  })),
);
</script>

<template>
  <div class="recommendations-page">
    <header class="rec-head">
      <h1>{{ t("推荐") }}</h1>
      <p class="rec-sub">{{ t("随应用提供的离线内容，不联网加载，也不访问任何网站。") }}</p>
    </header>

    <section v-for="group in groups" :key="group.id" class="rec-group">
      <div class="rec-group-head">
        <h2>{{ group.title }}</h2>
        <span class="rec-count">{{ group.items.length }}</span>
      </div>
      <div class="rec-grid">
        <article v-for="item in group.items" :key="item.id" class="rec-card">
          <div class="rec-thumb">
            <img
              v-if="item.image"
              :src="item.image.url"
              :alt="localize(item.image.alt) || localize(item.title)"
              loading="lazy"
            />
          </div>
          <div class="rec-body">
            <h3 v-if="localize(item.title)" class="rec-title">{{ localize(item.title) }}</h3>
            <p v-if="item.summary" class="rec-summary">{{ localize(item.summary) }}</p>
            <ul v-if="reasonLines(item.reason).length" class="rec-reason">
              <li v-for="(line, index) in reasonLines(item.reason)" :key="index">{{ line }}</li>
            </ul>
            <p v-if="item.compatibility" class="rec-compat">{{ localize(item.compatibility) }}</p>
            <a
              v-if="primaryAction(item)"
              class="rec-action"
              :href="primaryAction(item)!.url"
              target="_blank"
              rel="noreferrer"
            >
              {{ localize(primaryAction(item)!.label) || t("打开官网") }}
              <span aria-hidden="true">↗</span>
            </a>
          </div>
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* 推荐页（2026-11-17 重做）：与「自定义遥控器」保持同一套语言——
   12px 间距、10px 圆角、淡边框、卡片内 12px 内边距、操作收敛成一个按钮。 */
.recommendations-page {
  height: 100%;
  overflow-y: auto;
  box-sizing: border-box;
  padding: 24px 28px 32px;
}

.rec-head h1 {
  margin: 0 0 4px;
  font-size: 22px;
  font-weight: 650;
  letter-spacing: 0.01em;
  color: var(--text-primary);
}

.rec-sub {
  margin: 0 0 20px;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--muted);
}

.rec-group {
  margin-bottom: 26px;
}

.rec-group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.rec-group-head h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--muted);
}

.rec-count {
  min-width: 18px;
  height: 18px;
  padding: 0 6px;
  border-radius: 9px;
  background: var(--surface-muted, rgba(0, 0, 0, 0.05));
  font-size: 11px;
  line-height: 18px;
  text-align: center;
  color: var(--muted);
}

/* 列数按断点固定，不用 auto-fill：auto-fill 在 1600 宽的窗口会排成 4 列、
   每列被压到 370，卡片里的说明只剩两三个字一行，和设计稿完全不是一回事。 */
.rec-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 12px;
  max-width: 1400px;
}

@media (min-width: 780px) {
  .rec-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}

@media (min-width: 1180px) {
  .rec-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}

.rec-card {
  display: flex;
  gap: 14px;
  padding: 14px;
  border: 1px solid var(--border-subtle, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  background: var(--surface, #fff);
  transition: border-color 150ms ease, box-shadow 150ms ease, transform 150ms ease;
}

.rec-card:hover {
  border-color: var(--border-strong, rgba(0, 0, 0, 0.16));
  box-shadow: 0 4px 14px rgba(20, 40, 80, 0.07);
  transform: translateY(-1px);
}

.rec-thumb {
  flex: 0 0 88px;
  width: 88px;
  height: 88px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  overflow: hidden;
  background: var(--mapping-neutral, rgba(0, 0, 0, 0.04));
}

.rec-thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.rec-body {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.rec-title-row {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 8px;
  min-width: 0;
}

.rec-body .rec-title {
  margin: 0;
  /* 之前在 flex column 里写了 flex: 1 1 auto，标题会吃掉剩余高度 →
     1 行标题被撑到 59-71px，看起来标题和正文之间空了一大段。 */
  flex: 0 0 auto;
  min-width: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.35;
  color: var(--text-primary);
  /* 标题过长时最多两行截断（原来 nowrap 会把长标题直接切掉、看着像错位） */
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.rec-compat {
  margin: 0;
  font-size: 11px;
  line-height: 1.5;
  color: var(--muted);
  opacity: 0.85;
}

.rec-summary {
  margin: 0;
  word-break: break-word;
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--text-secondary, rgba(0, 0, 0, 0.7));
}

.rec-reason {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.rec-reason li {
  position: relative;
  padding-left: 12px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--muted);
}

.rec-reason li::before {
  content: "";
  position: absolute;
  left: 2px;
  top: 7px;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--border-strong, rgba(0, 0, 0, 0.2));
}

.rec-action {
  align-self: flex-start;
  margin-top: 2px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 10px;
  border-radius: 7px;
  border: 1px solid var(--border-subtle, rgba(0, 0, 0, 0.12));
  background: transparent;
  font-size: 12px;
  color: var(--accent-text, var(--primary, #4c6ef5));
  text-decoration: none;
  transition: background 150ms ease, border-color 150ms ease;
}

.rec-action:hover {
  background: var(--surface-muted, rgba(0, 0, 0, 0.04));
  border-color: var(--border-strong, rgba(0, 0, 0, 0.2));
  text-decoration: none;
}
</style>
