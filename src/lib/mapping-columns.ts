/**
 * 按键页画布的两列排法（2026-11-12 用户定稿）。
 *
 * 用户口径（原话）："新增的按钮应该从左边上边往左下依次排列。排满了现有的窗口
 * 以后再去排右边从上往下一直排；如果按键很多，现有的高度排满了以后还要再增加
 * 按钮，这时候在左边增加一个、右边增加一个，左右交替增加，让整个画面更加协调，
 * 两边的按钮数量一致。"
 *
 * 翻译成规则：
 * 1. 现有画布高度能放 `capacity` 张卡 → 先排左列，从第一行往下；
 * 2. 左列排满再排右列，同样从上往下；
 * 3. 两列都排满（数量 > 2×capacity）时画布**长高一行**，左右各加一张 ——
 *    所以两列数量永远相等或相差 1，不会出现一边排到窗口外、另一边空着。
 *
 * 这里是纯计算，画布高度、行距、卡片尺寸都由调用方传入，便于单测和复用
 * （ButtonsPage 只负责把结果换算成像素与归一化坐标）。
 */

export interface PlanColumnsOptions {
  /** 一列在**基础画布高度**下能放几张卡。 */
  capacity: number;
  /** 基础画布高度（px）：一列放得下时画布保持这个高度，不因按键少而变矮。 */
  baseHeight: number;
  /** 上下留白（px）。 */
  padding: number;
  /** 卡片高度（px）。 */
  cardHeight: number;
  /** 卡片之间的最小间距（px）。 */
  gap: number;
}

export interface ColumnPlan {
  /** 画布需要几行。 */
  rows: number;
  /** 左列卡片数（= rows 与总数取小）。 */
  leftCount: number;
  /** 右列卡片数。 */
  rightCount: number;
  /** 画布高度（px）：不足一列时等于 baseHeight。 */
  canvasHeight: number;
}

/** 参与排布的最少行数：按键再少也按整列高度铺（画布不会缩）。 */
export function planColumns(count: number, options: PlanColumnsOptions): ColumnPlan {
  const total = Math.max(0, Math.floor(count));
  const capacity = Math.max(1, Math.floor(options.capacity));
  const rows = Math.max(capacity, Math.ceil(total / 2));
  const leftCount = Math.min(total, rows);
  const rightCount = total - leftCount;
  const grown = options.padding * 2 + rows * (options.cardHeight + options.gap) - options.gap;
  return {
    rows,
    leftCount,
    rightCount,
    canvasHeight: rows <= capacity ? options.baseHeight : grown,
  };
}

/**
 * 第 `index` 行卡片**中心**的纵坐标（px）：首行贴 `padding`，末行贴画布底，
 * 中间均分 —— 行距随行数变化，卡片高度保持不变，永远不会互相压住。
 */
export function columnRowCenter(
  index: number,
  rows: number,
  canvasHeight: number,
  padding: number,
  cardHeight: number,
): number {
  const span = canvasHeight - padding * 2 - cardHeight;
  const pitch = rows > 1 ? span / (rows - 1) : 0;
  return padding + cardHeight / 2 + pitch * index;
}
