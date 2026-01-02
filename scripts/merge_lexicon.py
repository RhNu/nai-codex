#!/usr/bin/env python3
"""
合并词库脚本：将 danbooru.csv 中的权重数据合并到 JSON 词库文件中，
生成单个超级 JSON 文件用于编译时嵌入。

处理规则：
- 空格和下划线等价: "fake animal ears" == "fake_animal_ears"
- 大小写不敏感
- 匹配到的条目添加 weight 字段
- CSV 中未匹配的条目收集到 other 分类
"""

import json
import csv
from pathlib import Path
from typing import Dict, Tuple
from collections import defaultdict

# 路径配置
SCRIPT_DIR = Path(__file__).parent
ASSETS_DIR = SCRIPT_DIR.parent / "assets"
JSON_DIR = ASSETS_DIR / "json"
CSV_FILE = ASSETS_DIR / "danbooru.csv"
OUTPUT_FILE = ASSETS_DIR / "lexicon.json"


def normalize_tag(tag: str) -> str:
    """标准化标签：转小写，下划线转空格"""
    return tag.lower().replace("_", " ").strip()


def load_csv_weights() -> Dict[str, Tuple[int, str, str]]:
    """加载 CSV 文件，返回 {normalized_tag: (weight, chinese_translation, original_tag)}"""
    weights = {}
    with open(CSV_FILE, "r", encoding="utf-8") as f:
        reader = csv.reader(f)
        for row in reader:
            if len(row) >= 2:
                tag = row[0]
                weight = int(row[1]) if row[1].isdigit() else 0
                chinese = row[2] if len(row) > 2 else ""
                norm_tag = normalize_tag(tag)
                # 保留权重最高的版本（如果有重复）
                if norm_tag not in weights or weight > weights[norm_tag][0]:
                    weights[norm_tag] = (weight, chinese, tag)  # 保留原始tag
    return weights


def load_json_lexicon(filepath: Path) -> Dict[str, Dict[str, str]]:
    """加载单个 JSON 词库文件"""
    with open(filepath, "r", encoding="utf-8") as f:
        return json.load(f)


def process_lexicons():
    """处理所有词库文件，生成单个超级 JSON"""
    print("加载 CSV 权重数据...")
    csv_weights = load_csv_weights()
    print(f"CSV 中共有 {len(csv_weights)} 个唯一标签")

    # 记录已匹配的标签
    matched_tags = set()

    # 最终输出结构
    output = {
        "categories": [],
        "stats": {
            "total_tags": 0,
            "categorized_tags": 0,
            "uncategorized_tags": 0,
            "matched_weights": 0
        }
    }

    # 处理每个 JSON 文件
    json_files = sorted(JSON_DIR.glob("*.json"))
    print(f"找到 {len(json_files)} 个 JSON 词库文件")

    total_tags = 0
    matched_count = 0
    skipped_non_ascii_keys = 0
    skipped_null_values = 0

    for json_file in json_files:
        print(f"\n处理: {json_file.name}")
        lexicon = load_json_lexicon(json_file)

        category_name = json_file.stem
        category_data = {
            "name": category_name,
            "subcategories": []
        }

        for subcat_name, tags in lexicon.items():
            subcat_data = {
                "name": subcat_name,
                "tags": []
            }

            for tag, chinese in tags.items():
                # 清理掉非 ASCII 的键和空值，避免写出不可预期数据
                if not isinstance(tag, str) or not tag.isascii():
                    skipped_non_ascii_keys += 1
                    continue

                if chinese is None:
                    skipped_null_values += 1
                    continue

                total_tags += 1
                norm_tag = normalize_tag(tag)

                entry = {
                    "tag": tag,
                    "zh": chinese
                }

                if norm_tag in csv_weights:
                    weight, csv_chinese, original_csv_tag = csv_weights[norm_tag]
                    entry["weight"] = weight
                    matched_tags.add(norm_tag)
                    matched_count += 1

                subcat_data["tags"].append(entry)

            # 按权重排序
            subcat_data["tags"].sort(
                key=lambda x: x.get("weight", 0),
                reverse=True
            )
            category_data["subcategories"].append(subcat_data)

        output["categories"].append(category_data)
        print(f"  {category_name}: {sum(len(s['tags']) for s in category_data['subcategories'])} 标签")

    print(f"\nJSON 词库共 {total_tags} 个标签，匹配到权重 {matched_count} 个")

    if skipped_non_ascii_keys or skipped_null_values:
        print(
            "过滤无效记录: 非ASCII键 {0} 个，空值 {1} 个".format(
                skipped_non_ascii_keys,
                skipped_null_values
            )
        )

    # 收集未匹配的 CSV 条目到 other 分类
    print("\n生成 other 分类 (未分类标签)...")
    other_by_letter = defaultdict(list)

    unmatched_count = 0
    for norm_tag, (weight, chinese, original_tag) in csv_weights.items():
        if norm_tag not in matched_tags:
            unmatched_count += 1
            # 按首字母分类
            first_char = original_tag[0].upper() if original_tag else "_"
            if not first_char.isalpha():
                first_char = "#"

            other_by_letter[first_char].append({
                "tag": original_tag,
                "zh": chinese if chinese else original_tag,
                "weight": weight
            })

    # 构建 other 分类
    other_category = {
        "name": "other",
        "subcategories": []
    }

    for letter in sorted(other_by_letter.keys()):
        tags = other_by_letter[letter]
        tags.sort(key=lambda x: x.get("weight", 0), reverse=True)
        other_category["subcategories"].append({
            "name": letter,
            "tags": tags
        })

    output["categories"].append(other_category)
    print(f"未分类标签: {unmatched_count} 个")

    # 更新统计信息
    output["stats"]["total_tags"] = total_tags + unmatched_count
    output["stats"]["categorized_tags"] = total_tags
    output["stats"]["uncategorized_tags"] = unmatched_count
    output["stats"]["matched_weights"] = matched_count

    # 写入单个输出文件
    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, separators=(',', ':'))

    file_size = OUTPUT_FILE.stat().st_size
    print(f"\n输出文件: {OUTPUT_FILE}")
    print(f"文件大小: {file_size / 1024 / 1024:.2f} MB")
    print("\n完成!")


if __name__ == "__main__":
    process_lexicons()
