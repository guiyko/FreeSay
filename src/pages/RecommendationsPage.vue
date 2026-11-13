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
      <p class="muted">
        {{ t("以下内容随应用提供，不联网加载，也不访问任何网站。") }}
      </p>
    </header>

    <section v-for="group in groups" :key="group.id" class="rec-group">
      <h2>{{ group.title }}</h2>
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
            <h3>{{ localize(item.title) }}</h3>
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
            </a>
          </div>
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped>
.recommendations-page {
  height: 100%;
  overflow-y: auto;
  box-sizing: border-box;
  padding: 20px 24px 32px;
}

.rec-head h1 {
  margin: 0 0 6px;
  font-size: 20px;
  font-weight: 600;
}

.rec-head .muted {
  margin: 0 0 18px;
  font-size: 12px;
  color: var(--muted);
}

.rec-group {
  margin-bottom: 24px;
}

.rec-group h2 {
  margin: 0 0 10px;
  font-size: 14px;
  font-weight: 600;
  color: var(--muted);
}

.rec-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 12px;
}

.rec-card {
  display: flex;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border-strong);
  border-radius: 10px;
  background: var(--surface);
}

.rec-thumb {
  flex: 0 0 72px;
  width: 72px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  overflow: hidden;
  background: var(--mapping-neutral);
}

.rec-thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.rec-body {
  flex: 1;
  min-width: 0;
}

.rec-body h3 {
  margin: 0 0 4px;
  font-size: 13px;
  font-weight: 600;
}

.rec-summary {
  margin: 0 0 6px;
  font-size: 12px;
  line-height: 1.4;
  color: var(--muted);
}

.rec-reason {
  margin: 0 0 6px;
  padding-left: 16px;
  font-size: 12px;
  line-height: 1.5;
}

.rec-compat {
  margin: 0 0 6px;
  font-size: 11px;
  color: var(--muted);
}

.rec-action {
  font-size: 12px;
  color: var(--accent-text);
  text-decoration: none;
}

.rec-action:hover {
  text-decoration: underline;
}
</style>
