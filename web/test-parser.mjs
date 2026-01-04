import { parsePrompt } from './src/utils/promptParser.ts';

// 测试单斜杠
const input1 = '1girl, a/b, c / d';
const result1 = parsePrompt(input1);
const comments1 = result1.spans.filter((s) => s.type === 'comment');
console.log(
  '单斜杠测试:',
  comments1.length === 0 ? 'PASS' : 'FAIL',
  '(注释数:',
  comments1.length,
  ')',
);

// 测试三斜杠
const input2 = 'hello ///content// world';
const result2 = parsePrompt(input2);
const comments2 = result2.spans.filter((s) => s.type === 'comment');
console.log(
  '三斜杠测试:',
  comments2.length === 1 ? 'PASS' : 'FAIL',
  '(注释数:',
  comments2.length,
  ')',
);
if (comments2.length > 0) {
  const commentText = input2.slice(comments2[0].start, comments2[0].end);
  console.log('  注释内容:', commentText);
}

// 测试正常注释
const input3 = '1girl, //this is comment//, blue hair';
const result3 = parsePrompt(input3);
const comments3 = result3.spans.filter((s) => s.type === 'comment');
console.log(
  '正常注释测试:',
  comments3.length === 1 ? 'PASS' : 'FAIL',
  '(注释数:',
  comments3.length,
  ')',
);
