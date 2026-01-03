/**
 * NAI Prompt Parser - 前端纯本地解析
 *
 * NAI 支持的权重语法:
 * - `{tag}` - 增强 1.05 倍
 * - `{{tag}}` - 增强 1.05^2 倍，以此类推
 * - `[tag]` - 减弱，除以 1.05
 * - `[[tag]]` - 减弱，除以 1.05^2，以此类推
 * - `1.5::tag1, tag2 ::` - 冒号权重语法，乘以指定数值直到遇到 `::` 结束
 */

/** 权重倍数常量 */
const WEIGHT_MULTIPLIER = 1.05;

export type HighlightSpanType =
  | 'text'
  | 'comma'
  | 'whitespace'
  | 'brace'
  | 'bracket'
  | 'weight_num'
  | 'weight_end'
  | 'snippet'
  | 'newline';

export interface HighlightSpan {
  /** 起始位置（字符索引） */
  start: number;
  /** 结束位置（字符索引） */
  end: number;
  /** 权重: 1.0 为正常, >1 为增强, <1 为减弱 */
  weight: number;
  /** span 类型 */
  type: HighlightSpanType;
}

export interface ParseResult {
  spans: HighlightSpan[];
  /** 未闭合的 {} 数量 */
  unclosedBraces: number;
  /** 未闭合的 [] 数量 */
  unclosedBrackets: number;
  /** 是否有未结束的冒号权重 */
  unclosedWeight: boolean;
}

/**
 * 解析 NAI 提示词语法，返回高亮 spans
 */
export function parsePrompt(input: string): ParseResult {
  const spans: HighlightSpan[] = [];
  const len = input.length;

  // 状态跟踪
  let braceDepth = 0; // {} 深度
  let bracketDepth = 0; // [] 深度
  let colonWeight: number | null = null; // 当前冒号权重

  let pos = 0;

  // 安全保护：最大迭代次数，防止无限循环
  const maxIterations = len * 2 + 100;
  let iterations = 0;

  /** 计算当前权重 */
  function calculateWeight(): number {
    let weight = 1.0;

    // 应用 {} 增强
    if (braceDepth > 0) {
      weight *= Math.pow(WEIGHT_MULTIPLIER, braceDepth);
    }

    // 应用 [] 减弱
    if (bracketDepth > 0) {
      weight /= Math.pow(WEIGHT_MULTIPLIER, bracketDepth);
    }

    // 应用冒号权重
    if (colonWeight !== null) {
      weight *= colonWeight;
    }

    return weight;
  }

  /** 尝试解析冒号权重开始 `number::` */
  function tryParseWeightStart(): { value: number; end: number } | null {
    let p = pos;
    let numStr = '';

    // 可选负号
    if (p < len && input[p] === '-') {
      numStr += '-';
      p++;
    }

    // 收集数字部分
    let hasDigit = false;
    let hasDot = false;

    while (p < len) {
      const ch = input[p]!;
      if (ch >= '0' && ch <= '9') {
        numStr += ch;
        hasDigit = true;
        p++;
      } else if (ch === '.' && !hasDot) {
        numStr += ch;
        hasDot = true;
        p++;
      } else {
        break;
      }
    }

    if (!hasDigit) return null;

    // 检查是否有 `::`
    if (p + 1 < len && input[p] === ':' && input[p + 1] === ':') {
      const value = parseFloat(numStr);
      if (isNaN(value)) return null;
      return { value, end: p + 2 };
    }

    return null;
  }

  /** 尝试解析 snippet 引用 `<snippet:name>` */
  function tryParseSnippetRef(): { name: string; end: number } | null {
    const prefix = '<snippet:';
    if (input.slice(pos, pos + prefix.length) !== prefix) {
      return null;
    }

    let p = pos + prefix.length;
    let name = '';

    while (p < len) {
      const ch = input[p];
      if (ch === '>') {
        return { name, end: p + 1 };
      }
      if (ch === '<' || ch === '\n') {
        // 无效的 snippet 引用
        return null;
      }
      name += ch;
      p++;
    }

    return null;
  }

  while (pos < len) {
    // 安全保护：防止无限循环
    iterations++;
    if (iterations > maxIterations) {
      console.warn('[promptParser] 超过最大迭代次数，中止解析');
      break;
    }

    const ch = input[pos]!;

    // 检查换行
    if (ch === '\n') {
      spans.push({
        start: pos,
        end: pos + 1,
        weight: 1.0,
        type: 'newline',
      });
      pos++;
      continue;
    }

    // 检查 \r\n
    if (ch === '\r') {
      if (pos + 1 < len && input[pos + 1] === '\n') {
        spans.push({
          start: pos,
          end: pos + 2,
          weight: 1.0,
          type: 'newline',
        });
        pos += 2;
      } else {
        spans.push({
          start: pos,
          end: pos + 1,
          weight: 1.0,
          type: 'newline',
        });
        pos++;
      }
      continue;
    }

    // 检查冒号权重语法: `number::`
    if ((ch >= '0' && ch <= '9') || ch === '-' || ch === '.') {
      const weightStart = tryParseWeightStart();
      if (weightStart) {
        spans.push({
          start: pos,
          end: weightStart.end,
          weight: weightStart.value,
          type: 'weight_num',
        });
        colonWeight = weightStart.value;
        pos = weightStart.end;
        continue;
      }
    }

    // 检查权重结束 `::`
    if (ch === ':' && pos + 1 < len && input[pos + 1] === ':') {
      if (colonWeight !== null) {
        // 正常的权重结束符
        spans.push({
          start: pos,
          end: pos + 2,
          weight: 1.0,
          type: 'weight_end',
        });
        colonWeight = null;
        pos += 2;
        continue;
      } else {
        // colonWeight 为 null 时，:: 作为普通文本处理
        // 避免无限循环
        spans.push({
          start: pos,
          end: pos + 2,
          weight: 1.0,
          type: 'text',
        });
        pos += 2;
        continue;
      }
    }

    // 检查 `{`
    if (ch === '{') {
      braceDepth++;
      spans.push({
        start: pos,
        end: pos + 1,
        weight: Math.pow(WEIGHT_MULTIPLIER, braceDepth),
        type: 'brace',
      });
      pos++;
      continue;
    }

    // 检查 `}`
    if (ch === '}') {
      const prevWeight = Math.pow(WEIGHT_MULTIPLIER, braceDepth);
      braceDepth = Math.max(braceDepth - 1, 0);
      spans.push({
        start: pos,
        end: pos + 1,
        weight: prevWeight,
        type: 'brace',
      });
      pos++;
      continue;
    }

    // 检查 `[`
    if (ch === '[') {
      bracketDepth++;
      spans.push({
        start: pos,
        end: pos + 1,
        weight: 1 / Math.pow(WEIGHT_MULTIPLIER, bracketDepth),
        type: 'bracket',
      });
      pos++;
      continue;
    }

    // 检查 `]`
    if (ch === ']') {
      const prevWeight = 1 / Math.pow(WEIGHT_MULTIPLIER, bracketDepth);
      bracketDepth = Math.max(bracketDepth - 1, 0);
      spans.push({
        start: pos,
        end: pos + 1,
        weight: prevWeight,
        type: 'bracket',
      });
      pos++;
      continue;
    }

    // 检查逗号
    if (ch === ',') {
      spans.push({
        start: pos,
        end: pos + 1,
        weight: 1.0,
        type: 'comma',
      });
      pos++;
      continue;
    }

    // 检查空白（不含换行）
    if (ch === ' ' || ch === '\t') {
      const start = pos;
      while (pos < len && (input[pos] === ' ' || input[pos] === '\t')) {
        pos++;
      }
      spans.push({
        start,
        end: pos,
        weight: 1.0,
        type: 'whitespace',
      });
      continue;
    }

    // 检查 snippet 引用: `<snippet:name>`
    if (ch === '<') {
      const snippetRef = tryParseSnippetRef();
      if (snippetRef) {
        spans.push({
          start: pos,
          end: snippetRef.end,
          weight: calculateWeight(),
          type: 'snippet',
        });
        pos = snippetRef.end;
        continue;
      }
      // 不是有效的 snippet 引用，把 '<' 当作普通字符处理
      spans.push({
        start: pos,
        end: pos + 1,
        weight: calculateWeight(),
        type: 'text',
      });
      pos++;
      continue;
    }

    // 普通文本 - 收集直到遇到特殊字符
    const textStart = pos;
    while (pos < len) {
      const c = input[pos]!;
      if (
        c === '{' ||
        c === '}' ||
        c === '[' ||
        c === ']' ||
        c === ',' ||
        c === '\n' ||
        c === '\r' ||
        c === '<' ||
        c === ' ' ||
        c === '\t'
      ) {
        break;
      }
      // 检查是否是 `::` 权重结束
      if (c === ':' && pos + 1 < len && input[pos + 1] === ':') {
        break;
      }
      // 检查是否是权重开始
      if ((c >= '0' && c <= '9') || c === '-' || c === '.') {
        const saved = pos;
        const tmp = tryParseWeightStart();
        if (tmp) {
          pos = saved; // 恢复位置，让外层处理
          break;
        }
      }
      pos++;
    }

    if (pos > textStart) {
      spans.push({
        start: textStart,
        end: pos,
        weight: calculateWeight(),
        type: 'text',
      });
    }
  }

  return {
    spans,
    unclosedBraces: braceDepth,
    unclosedBrackets: bracketDepth,
    unclosedWeight: colonWeight !== null,
  };
}
