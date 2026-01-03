/**
 * useImageMetadata - 图片元数据解析 composable
 *
 * 功能：
 * - 解析图片 EXIF 数据
 * - 支持 NovelAI, ComfyUI, Stable Diffusion, Midjourney, Illustrious XL 等格式
 * - 提取正向提示词、负向提示词、模型、采样器等参数
 *
 * Credit: https://github.com/looyun/spell
 */
import { ref, type Ref, computed } from 'vue';
import ExifReader from 'exifreader';

/** 支持的生成器类型 */
export type GeneratorType =
  | 'NovelAI'
  | 'ComfyUI'
  | 'Stable Diffusion'
  | 'Midjourney'
  | 'Illustrious XL'
  | 'Unknown';

/** 图片尺寸 */
export interface ImageDimensions {
  width: number;
  height: number;
}

/** 解析后的元数据 */
export interface ParsedMetadata {
  /** 生成器类型 */
  generator: GeneratorType;
  /** 图片尺寸 */
  dimensions?: ImageDimensions;
  /** 模型名称 */
  model?: string;
  /** CFG Scale */
  cfg?: number | string;
  /** 采样步数 */
  steps?: number | string;
  /** 随机种子 */
  seed?: number | string;
  /** 采样器 */
  sampler?: string;
  /** 调度器 */
  scheduler?: string;
  /** 正向提示词 */
  positivePrompt?: string;
  /** 负向提示词 */
  negativePrompt?: string;
  /** 原始参数字符串 */
  rawParameters?: string;
  /** 错误信息 */
  error?: string;
  /** 原始标签数据 */
  rawTags?: Record<string, unknown>;
}

export interface UseImageMetadataOptions {
  /** 自动获取图片尺寸 */
  autoGetDimensions?: boolean;
}

export interface UseImageMetadataReturn {
  /** 解析后的元数据 */
  metadata: Ref<ParsedMetadata | null>;
  /** 是否正在解析 */
  loading: Ref<boolean>;
  /** 错误信息 */
  error: Ref<string | null>;
  /** 解析图片文件 */
  parseFile: (file: File) => Promise<ParsedMetadata | null>;
  /** 清除元数据 */
  clear: () => void;
  /** 是否有元数据 */
  hasMetadata: Ref<boolean>;
}

/**
 * 获取图片尺寸
 */
function getImageDimensions(file: File): Promise<ImageDimensions> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      resolve({
        width: img.width,
        height: img.height,
      });
      URL.revokeObjectURL(img.src);
    };
    img.onerror = () => {
      reject(new Error('无法获取图片尺寸'));
      URL.revokeObjectURL(img.src);
    };
    img.src = URL.createObjectURL(file);
  });
}

/**
 * 检测生成器类型
 */
function detectGenerator(exif: Record<string, unknown>): GeneratorType {
  const software = (exif.Software as { value?: string })?.value;
  const description = (exif.Description as { value?: string })?.value;
  const userComment = (exif.UserComment as { value?: string })?.value;

  // Midjourney 特征
  if (
    software?.includes('Midjourney') ||
    description?.includes('--') ||
    userComment?.includes('--')
  ) {
    return 'Midjourney';
  }

  // ComfyUI 特征: 包含 prompt 和 workflow
  if (exif.prompt || exif.workflow || exif.generation_data) {
    return 'ComfyUI';
  }

  // NovelAI 特征
  if (software === 'NovelAI') {
    return 'NovelAI';
  }

  // Illustrious XL 特征
  if ((exif.generate_info as { value?: string })?.value) {
    return 'Illustrious XL';
  }

  // Stable Diffusion 特征: 包含 parameters
  if ((exif.parameters as { value?: string })?.value) {
    return 'Stable Diffusion';
  }

  return 'Unknown';
}

/**
 * 解析 Illustrious XL 格式
 */
function parseIllustriousXL(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    const generateInfo = (exif.generate_info as { value?: string })?.value || '{}';
    const params = JSON.parse(generateInfo);

    return {
      model: params.checkpoint || 'Unknown',
      cfg: params.cfgScale,
      steps: params.steps,
      seed: params.seed,
      sampler: params.samplerName,
      scheduler: params.scheduler,
      positivePrompt: params.prompt || '',
      negativePrompt: params.negativePrompt || '',
    };
  } catch (error) {
    console.error('解析 Illustrious XL 失败:', error);
    return {};
  }
}

/**
 * 解析 NovelAI 格式
 */
function parseNovelAI(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    const comment = (exif.Comment as { value?: string })?.value || '{}';
    const params = JSON.parse(comment);

    return {
      model: (exif.Source as { value?: string })?.value || 'Unknown',
      cfg: params.scale,
      steps: params.steps,
      seed: params.seed,
      sampler: params.sampler,
      scheduler: params.noise_schedule,
      positivePrompt: params.prompt || '',
      negativePrompt: params.uc || '',
    };
  } catch (error) {
    console.error('解析 NovelAI 失败:', error);
    return {};
  }
}

/**
 * 递归获取 prompt 节点
 */
function getPromptNode(
  node: Record<string, unknown> | undefined,
  datas: Record<string, Record<string, unknown> | undefined>,
): Record<string, unknown> | null {
  if (!node) return null;

  const classType = node.class_type as string;
  const inputs = node.inputs as Record<string, unknown> | undefined;

  if (classType === 'CLIPTextEncode' || classType === 'smZ CLIPTextEncode') {
    if (typeof inputs?.text === 'string') {
      return node;
    }
    if (Array.isArray(inputs?.text)) {
      const key = inputs.text[0] as string;
      return getPromptNode(datas[key], datas);
    }
  }
  if (inputs?.conditioning) {
    const key = (inputs.conditioning as unknown[])[0] as string;
    return getPromptNode(datas[key], datas);
  }
  return node;
}

/**
 * 过滤 undefined 值
 */
function filterUndefined<T extends Record<string, unknown>>(obj: T): Partial<ParsedMetadata> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    if (value !== undefined) {
      result[key] = value;
    }
  }
  return result as Partial<ParsedMetadata>;
}

/**
 * 解析 ComfyUI generation_data 格式
 */
function parseComfyUIGenerationData(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    let generationDataValue = (exif.generation_data as { value?: string })?.value || '{}';
    // 修复 null 字符问题
    // eslint-disable-next-line no-control-regex
    generationDataValue = generationDataValue.replace(/\u0000/g, '');
    const data = JSON.parse(generationDataValue);

    return filterUndefined({
      model: data.baseModel?.modelFileName || 'Unknown',
      cfg: data.cfgScale,
      steps: data.steps,
      seed: data.seed,
      sampler: data.ksamplerName || data.samplerName,
      scheduler: data.schedule,
      positivePrompt: data.prompt || '',
      negativePrompt: data.uc || '',
    });
  } catch (error) {
    console.error('解析 ComfyUI generation_data 失败:', error);
    return {};
  }
}

/**
 * 解析 ComfyUI prompt 格式
 */
function parseComfyUIPrompts(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    let promptValue = (exif.prompt as { value?: string })?.value || '{}';
    // 修复 NaN 问题
    promptValue = promptValue.replace(/NaN/g, '0');
    const data = JSON.parse(promptValue) as Record<string, Record<string, unknown>>;

    let samplerInput: Record<string, unknown> | null = null;
    let schedulerInput: Record<string, unknown> | null = null;
    let checkPointInput: Record<string, unknown> | null = null;
    let unetInput: Record<string, unknown> | null = null;
    let positivePromptInput: Record<string, unknown> | null = null;
    let negativePromptInput: Record<string, unknown> | null = null;

    for (const key in data) {
      const node = data[key];
      const inputs = node?.inputs as Record<string, unknown>;
      const classType = node?.class_type as string;

      if (inputs) {
        if (
          classType === 'KSampler' ||
          classType === 'KSampler (Efficient)' ||
          classType === 'BasicScheduler'
        ) {
          if (samplerInput === null && inputs.sampler_name) {
            samplerInput = inputs;
          }
          if (schedulerInput === null && inputs.scheduler) {
            schedulerInput = inputs;
          }
        } else if (classType === 'SamplerCustomAdvanced') {
          if (samplerInput === null && inputs.sampler) {
            const samplerKey = (inputs.sampler as unknown[])[0] as string;
            samplerInput = (data[samplerKey]?.inputs as Record<string, unknown>) || null;
          }
          if (schedulerInput === null && inputs.sigmas) {
            const sigmasKey = (inputs.sigmas as unknown[])[0] as string;
            schedulerInput = (data[sigmasKey]?.inputs as Record<string, unknown>) || null;
          }
          if (positivePromptInput === null && inputs.guider) {
            const guiderKey = (inputs.guider as unknown[])[0] as string;
            const guiderNode = getPromptNode(data[guiderKey], data);
            positivePromptInput = guiderNode?.inputs as Record<string, unknown> | null;
          }
        } else if (classType === 'CheckpointLoaderSimple' || classType === 'easy a1111Loader') {
          checkPointInput = inputs;
        } else if (classType === 'UNETLoader') {
          unetInput = inputs;
        } else if (classType === 'WeiLinComfyUIPromptAllInOneGreat') {
          positivePromptInput = inputs;
        } else if (classType === 'WeiLinComfyUIPromptAllInOneNeg') {
          negativePromptInput = inputs;
        }
      }
    }

    // 从 sampler 节点中获取正向和负向提示
    if (positivePromptInput === null && samplerInput?.positive) {
      const positiveKey = (samplerInput.positive as unknown[])[0] as string;
      const promptNode = getPromptNode(data[positiveKey], data);
      if (promptNode) {
        positivePromptInput = promptNode.inputs as Record<string, unknown>;
      }
    }
    if (negativePromptInput === null && samplerInput?.negative) {
      const negativeKey = (samplerInput.negative as unknown[])[0] as string;
      const negativePromptNode = getPromptNode(data[negativeKey], data);
      if (negativePromptNode) {
        negativePromptInput = negativePromptNode.inputs as Record<string, unknown>;
      }
    }

    const positivePrompt =
      (positivePromptInput?.text as string) ||
      (positivePromptInput?.positive as string) ||
      (positivePromptInput?.tags as string) ||
      '';
    const negativePrompt =
      (negativePromptInput?.text as string) ||
      (negativePromptInput?.positive as string) ||
      (negativePromptInput?.tags as string) ||
      '';

    return filterUndefined({
      model:
        (checkPointInput?.ckpt_name as string) || (unetInput?.unet_name as string) || 'Unknown',
      cfg: samplerInput?.cfg as number | undefined,
      steps: schedulerInput?.steps as number | undefined,
      seed: samplerInput?.seed as number | undefined,
      sampler: samplerInput?.sampler_name as string | undefined,
      scheduler: schedulerInput?.scheduler as string | undefined,
      positivePrompt,
      negativePrompt,
    });
  } catch (error) {
    console.error('解析 ComfyUI prompt 失败:', error);
    return {};
  }
}

/**
 * 解析 ComfyUI workflow 格式
 */
function parseComfyUIWorkflow(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    const workflowValue = (exif.workflow as { value?: string })?.value || '{}';
    const workflow = JSON.parse(workflowValue);

    if (!workflow?.nodes) {
      return {};
    }

    const nodes = workflow.nodes as Record<string, unknown>[];

    // 查找采样器节点
    const samplerNode =
      nodes.find((n) => n.type === 'KSampler (Efficient)') ||
      nodes.find((n) => n.type === 'KSampler') ||
      nodes.find((n) => n.type === 'KSampler (Advanced)');

    if (!samplerNode) {
      return {};
    }

    const widgetsValues = samplerNode.widgets_values as unknown[];
    let seed: number | undefined;
    let steps: number | undefined;
    let cfg: number | undefined;
    let samplerName: string | undefined;
    let scheduler: string | undefined;

    if (samplerNode.type === 'KSampler (Efficient)' && widgetsValues?.length > 5) {
      seed = widgetsValues[0] as number;
      steps = widgetsValues[2] as number;
      cfg = widgetsValues[3] as number;
      samplerName = widgetsValues[4] as string;
      scheduler = widgetsValues[5] as string;
    } else if (samplerNode.type === 'KSampler' && widgetsValues?.length > 4) {
      seed = widgetsValues[0] as number;
      steps = widgetsValues[1] as number;
      cfg = widgetsValues[2] as number;
      samplerName = widgetsValues[3] as string;
      scheduler = widgetsValues[4] as string;
    }

    // 查找正向和负向提示词节点
    let positivePrompt: string | undefined;
    let negativePrompt: string | undefined;

    const findSourceNodeForLink = (targetLinkId: number) => {
      for (const node of nodes) {
        const outputs = node.outputs as Array<{ links?: number[] }> | undefined;
        if (outputs) {
          for (const output of outputs) {
            if (output.links?.includes(targetLinkId)) {
              return node;
            }
          }
        }
      }
      return null;
    };

    const findTextEncodeNode = (linkId: number, depth = 0): Record<string, unknown> | null => {
      if (depth > 20) return null;
      const sourceNode = findSourceNodeForLink(linkId);
      if (!sourceNode) return null;
      if (sourceNode.type === 'CLIPTextEncode') {
        return sourceNode;
      }
      const inputs = sourceNode.inputs as Array<{ type?: string; link?: number }> | undefined;
      if (inputs) {
        const condInput = inputs.find((i) => i.type === 'CONDITIONING' && i.link);
        if (condInput?.link) {
          return findTextEncodeNode(condInput.link, depth + 1);
        }
      }
      return null;
    };

    const samplerInputs = samplerNode.inputs as Array<{ name?: string; link?: number }> | undefined;
    const positiveInput = samplerInputs?.find((i) => i.name === 'positive');
    const negativeInput = samplerInputs?.find((i) => i.name === 'negative');

    if (positiveInput?.link) {
      const textNode = findTextEncodeNode(positiveInput.link);
      if (textNode?.widgets_values) {
        positivePrompt = (textNode.widgets_values as string[])[0];
      }
    }

    if (negativeInput?.link) {
      const textNode = findTextEncodeNode(negativeInput.link);
      if (textNode?.widgets_values) {
        negativePrompt = (textNode.widgets_values as string[])[0];
      }
    }

    return filterUndefined({
      seed,
      steps,
      cfg,
      sampler: samplerName,
      scheduler,
      positivePrompt,
      negativePrompt,
    });
  } catch (error) {
    console.error('解析 ComfyUI workflow 失败:', error);
    return {};
  }
}

/**
 * 解析 ComfyUI 格式
 */
function parseComfyUI(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  try {
    // 尝试不同的解析方式
    if (exif.generation_data) {
      const result = parseComfyUIGenerationData(exif);
      if (result.positivePrompt || result.model) {
        return result;
      }
    }

    if (exif.prompt) {
      const result = parseComfyUIPrompts(exif);
      if (result.positivePrompt || result.model) {
        return result;
      }
    }

    if (exif.workflow) {
      return parseComfyUIWorkflow(exif);
    }

    return {};
  } catch (error) {
    console.error('解析 ComfyUI 失败:', error);
    return {};
  }
}

/**
 * 提取 Stable Diffusion 参数
 */
function extractSDParameters(params: string): Record<string, string> {
  const result: Record<string, string> = {};
  const patterns: Record<string, RegExp> = {
    steps: /Steps: (\d+)/,
    sampler: /Sampler: ([^,]+)/,
    cfg: /CFG scale: (\d+\.?\d*)/,
    seed: /Seed: (\d+)/,
    size: /Size: (\d+x\d+)/,
    scheduler: /Schedule type: (\w+)/,
    model: /Model: ([^,]+)/,
  };

  for (const [key, pattern] of Object.entries(patterns)) {
    const match = params.match(pattern);
    if (match?.[1]) {
      result[key] = match[1];
    }
  }

  return result;
}

/**
 * 解析 Stable Diffusion 格式
 */
function parseStableDiffusion(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  const params = (exif.parameters as { value?: string })?.value || '';
  if (!params) {
    return {};
  }

  // 尝试 JSON 格式
  if (params.includes('negative_prompt')) {
    try {
      const data = JSON.parse(params);
      return filterUndefined({
        model: data['Model'] || 'Unknown',
        cfg: data['guidance_scale'],
        steps: data['num_inference_steps'],
        seed: data['seed'],
        sampler: data['sampler'],
        scheduler: data['scheduler'],
        positivePrompt: data['prompt'] || '',
        negativePrompt: data['negative_prompt'] || '',
        rawParameters: params,
      });
    } catch {
      // 继续尝试其他格式
    }
  }

  // 标准 SD 格式
  const parts = params.split('Negative prompt:');
  const positivePrompt = parts[0]?.trim() || '';
  const negativePrompt = parts[1]?.split('Steps:')[0]?.trim() || '';
  const extracted = extractSDParameters(params);

  return filterUndefined({
    model: extracted.model || 'Stable Diffusion',
    cfg: extracted.cfg,
    steps: extracted.steps,
    seed: extracted.seed,
    sampler: extracted.sampler,
    scheduler: extracted.scheduler,
    positivePrompt,
    negativePrompt,
    rawParameters: params,
  });
}

/**
 * 解析 Midjourney 格式
 */
function parseMidjourney(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  let prompt = '';

  const description = (exif.Description as { value?: string })?.value;
  const userComment = exif.UserComment as { value?: string | number[] };
  const comment = (exif.Comment as { value?: string })?.value;

  if (description) {
    prompt = description;
  } else if (userComment?.value) {
    if (typeof userComment.value === 'string') {
      prompt = userComment.value;
    } else if (Array.isArray(userComment.value)) {
      prompt = String.fromCharCode(...userComment.value);
    }
  } else if (comment) {
    prompt = comment;
  }

  // 清理控制字符
  // eslint-disable-next-line no-control-regex
  prompt = prompt.replace(/[\u0000-\u001F]/g, '').trim();

  return {
    positivePrompt: prompt,
  };
}

/**
 * 解析通用格式
 */
function parseGeneric(exif: Record<string, unknown>): Partial<ParsedMetadata> {
  const userComment = exif.UserComment as { value?: number[] };
  if (userComment?.value && Array.isArray(userComment.value)) {
    try {
      const str = String.fromCharCode(...userComment.value);
      // eslint-disable-next-line no-control-regex
      const controlCharRegex = /[\u0000-\u001F]/g;
      const lines = str
        .trim()
        .split('\n')
        .map((line) => line.replace(controlCharRegex, ''))
        .filter((line) => line.trim() !== '');

      if (lines.length === 0) return {};

      // 处理最后一行作为配置
      const lastLine = lines.pop() || '';
      const result: Record<string, string> = {};

      lastLine.split(',').forEach((param) => {
        const [key, value] = param.split(':', 2).map((p) => p.trim());
        if (key && value) {
          result[key.trim()] = value.trim();
        }
      });

      return filterUndefined({
        model: result['Model'] || 'Unknown',
        cfg: result['CFG scale'],
        steps: result['Steps'],
        seed: result['Seed'],
        sampler: result['Sampler'],
        scheduler: result['Schedule type'],
        negativePrompt: lines.pop() || '',
        positivePrompt: lines.join('\n') || '',
      });
    } catch (error) {
      console.error('解析通用格式失败:', error);
    }
  }
  return {};
}

/**
 * 图片元数据解析 composable
 */
export function useImageMetadata(options: UseImageMetadataOptions = {}): UseImageMetadataReturn {
  const { autoGetDimensions = true } = options;

  const metadata = ref<ParsedMetadata | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const hasMetadata = computed(() => metadata.value !== null);

  /**
   * 解析图片文件
   */
  async function parseFile(file: File): Promise<ParsedMetadata | null> {
    if (!file.type.startsWith('image/')) {
      error.value = '请上传图片文件';
      return null;
    }

    loading.value = true;
    error.value = null;

    try {
      // 读取 EXIF 数据
      const exif = (await ExifReader.load(file)) as Record<string, unknown>;
      console.log('解析到的 EXIF:', exif);

      // 转换为普通对象
      const rawMetadata: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(exif)) {
        if (value && typeof value === 'object') {
          rawMetadata[key] = value;
        }
      }

      // 获取图片尺寸
      let dimensions: ImageDimensions | undefined;
      if (autoGetDimensions) {
        try {
          dimensions = await getImageDimensions(file);
        } catch {
          console.warn('无法获取图片尺寸');
        }
      }

      // 检测生成器类型
      const generator = detectGenerator(exif);
      console.log('检测到的生成器:', generator);

      // 根据不同工具解析元数据
      let parsedData: Partial<ParsedMetadata>;
      switch (generator) {
        case 'NovelAI':
          parsedData = parseNovelAI(exif);
          break;
        case 'ComfyUI':
          parsedData = parseComfyUI(exif);
          break;
        case 'Stable Diffusion':
          parsedData = parseStableDiffusion(exif);
          break;
        case 'Midjourney':
          parsedData = parseMidjourney(exif);
          break;
        case 'Illustrious XL':
          parsedData = parseIllustriousXL(exif);
          break;
        default:
          parsedData = parseGeneric(exif);
      }

      const result: ParsedMetadata = {
        generator,
        ...parsedData,
        rawTags: rawMetadata,
      };
      if (dimensions) {
        result.dimensions = dimensions;
      }

      metadata.value = result;
      return result;
    } catch (err) {
      console.error('解析失败:', err);
      error.value = err instanceof Error ? err.message : '解析失败';

      // 即使解析失败也尝试返回部分数据
      const result: ParsedMetadata = {
        generator: 'Unknown',
        error: error.value,
      };
      metadata.value = result;
      return result;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 清除元数据
   */
  function clear(): void {
    metadata.value = null;
    error.value = null;
  }

  return {
    metadata,
    loading,
    error,
    parseFile,
    clear,
    hasMetadata,
  };
}

/**
 * 直接解析图片文件（不使用响应式状态）
 */
export async function parseImageMetadata(file: File): Promise<ParsedMetadata | null> {
  if (!file.type.startsWith('image/')) {
    return null;
  }

  try {
    const exif = (await ExifReader.load(file)) as Record<string, unknown>;
    const rawMetadata: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(exif)) {
      if (value && typeof value === 'object') {
        rawMetadata[key] = value;
      }
    }

    let dimensions: ImageDimensions | undefined;
    try {
      dimensions = await getImageDimensions(file);
    } catch {
      // ignore
    }

    const generator = detectGenerator(exif);
    let parsedData: Partial<ParsedMetadata>;

    switch (generator) {
      case 'NovelAI':
        parsedData = parseNovelAI(exif);
        break;
      case 'ComfyUI':
        parsedData = parseComfyUI(exif);
        break;
      case 'Stable Diffusion':
        parsedData = parseStableDiffusion(exif);
        break;
      case 'Midjourney':
        parsedData = parseMidjourney(exif);
        break;
      case 'Illustrious XL':
        parsedData = parseIllustriousXL(exif);
        break;
      default:
        parsedData = parseGeneric(exif);
    }

    const result: ParsedMetadata = {
      generator,
      ...parsedData,
      rawTags: rawMetadata,
    };
    if (dimensions) {
      result.dimensions = dimensions;
    }

    return result;
  } catch (err) {
    console.error('解析失败:', err);
    return {
      generator: 'Unknown',
      error: err instanceof Error ? err.message : '解析失败',
    };
  }
}
