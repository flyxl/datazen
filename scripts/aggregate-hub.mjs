#!/usr/bin/env node
// scripts/aggregate-hub.mjs
// 方案 B 核心工具：扫描 docs/development/coordination/tracks/<track>/progress.md 及 bugs.md，
// 自动聚合生成 docs/development/coordination/hub.md。
// 各功能分支只维护自己 track 目录下的文件，禁止手动编辑 hub.md，彻底避免合并冲突。
//
// 用法:
//   node scripts/aggregate-hub.mjs [--check] [coordination-dir]

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');

const isCheck = process.argv.includes('--check');
const customDir = process.argv.find(arg => !arg.startsWith('--') && arg !== process.argv[0] && arg !== process.argv[1]);
const COORD_DIR = path.resolve(REPO_ROOT, customDir || 'docs/development/coordination');
const TRACKS_DIR = path.join(COORD_DIR, 'tracks');
const HUB_FILE = path.join(COORD_DIR, 'hub.md');

if (!fs.existsSync(COORD_DIR)) {
  if (isCheck) {
    console.log(`ℹ️ 协调目录不存在: ${COORD_DIR}（无需检查）`);
    process.exit(0);
  }
  console.log(`ℹ️ 协调目录不存在: ${COORD_DIR}（无活跃轨道）`);
  process.exit(0);
}

// 1. 尝试从现有 hub.md 或 plan.md 提取静态段落（波次记录、跨轨风险、R 阶段清单、头部元信息）
let existingHub = '';
if (fs.existsSync(HUB_FILE)) {
  existingHub = fs.readFileSync(HUB_FILE, 'utf8');
}

// 尝试寻找 plan.md
let planContent = '';
const planFiles = fs.readdirSync(COORD_DIR).filter(f => f.endsWith('-plan.md'));
if (planFiles.length > 0) {
  planContent = fs.readFileSync(path.join(COORD_DIR, planFiles[0]), 'utf8');
}

// 从 plan.md 提取 track -> 任务摘要 的映射
const taskMap = new Map();
if (planContent) {
  const planRows = planContent.match(/\|\s*\*\*?([a-zA-Z0-9_-]+)\*\*?\s*\|\s*[^|]+\|\s*([^|]+)\|/g);
  if (planRows) {
    for (const row of planRows) {
      const match = row.match(/\|\s*\*\*?([a-zA-Z0-9_-]+)\*\*?\s*\|\s*[^|]+\|\s*([^|]+)\|/);
      if (match) {
        taskMap.set(match[1].trim(), match[2].trim());
      }
    }
  }
}

// 解析单个 progress.md
function parseProgress(trackId, content) {
  const data = {
    track: trackId,
    task: taskMap.get(trackId) || '—',
    phase: '未开始',
    codingCommit: '—',
    testCommit: '—',
    mergeCommit: '—',
    agent: '—',
    worktree: '—',
    branch: `feature/${trackId}`,
    lastHeartbeat: '—',
    unresolvedBugs: 0,
  };

  const lines = content.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    const kvMatch = trimmed.match(/^[-*]\s*([^:]+):\s*(.+)$/);
    if (!kvMatch) continue;
    const key = kvMatch[1].trim().toLowerCase();
    const val = kvMatch[2].trim();

    if (key === 'phase' || key === '状态') data.phase = val;
    else if (key.includes('编码 commit') || key === 'coding_commit' || key === 'codingcommit') data.codingCommit = val;
    else if (key.includes('测试 commit') || key === 'test_commit' || key === 'testcommit') data.testCommit = val;
    else if (key.includes('合并 commit') || key === 'merge_commit' || key === 'mergecommit') data.mergeCommit = val;
    else if (key.includes('代理') || key.includes('agent')) data.agent = val;
    else if (key.includes('worktree')) data.worktree = val;
    else if (key.includes('branch') || key.includes('分支')) data.branch = val;
    else if (key.includes('心跳') || key.includes('heartbeat')) data.lastHeartbeat = val;
    else if (key.includes('task') || key.includes('任务')) data.task = val;
  }

  // 检查 bugs.md
  const bugsPath = path.join(TRACKS_DIR, trackId, 'bugs.md');
  if (fs.existsSync(bugsPath)) {
    const bugContent = fs.readFileSync(bugsPath, 'utf8');
    const openMatches = bugContent.match(/状态[:：]\s*(待验证|待修复|新建|OPEN)/gi);
    if (openMatches) {
      data.unresolvedBugs = openMatches.length;
    }
  }

  return data;
}

// 扫描所有 tracks
const tracks = [];
if (fs.existsSync(TRACKS_DIR)) {
  const entries = fs.readdirSync(TRACKS_DIR, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isDirectory()) {
      const trackId = entry.name;
      const progressPath = path.join(TRACKS_DIR, trackId, 'progress.md');
      if (fs.existsSync(progressPath)) {
        const content = fs.readFileSync(progressPath, 'utf8');
        tracks.push(parseProgress(trackId, content));
      } else {
        tracks.push({
          track: trackId,
          task: taskMap.get(trackId) || '—',
          phase: '未开始',
          codingCommit: '—',
          testCommit: '—',
          mergeCommit: '—',
          agent: '—',
          worktree: '—',
          branch: `feature/${trackId}`,
          lastHeartbeat: '—',
          unresolvedBugs: 0,
        });
      }
    }
  }
}

// 保持既有顺序，若现有 hub 存在总览表，按既有顺序排；否则按字母序排
if (existingHub) {
  const orderMatch = existingHub.match(/\|\s*([a-zA-Z0-9_-]+)\s*\|[^|]+\|/g);
  if (orderMatch) {
    const order = orderMatch.map(m => m.split('|')[1].trim());
    tracks.sort((a, b) => {
      const ia = order.indexOf(a.track);
      const ib = order.indexOf(b.track);
      if (ia !== -1 && ib !== -1) return ia - ib;
      if (ia !== -1) return -1;
      if (ib !== -1) return 1;
      return a.track.localeCompare(b.track);
    });
  }
}

// 提取静态/保留段落（波次记录、跨轨风险、R 阶段清单）
function extractSection(heading, fallback = '') {
  if (!existingHub) return fallback;
  const regex = new RegExp(`##\\s+${heading}([\\s\\S]*?)(?=\\n##\\s+|$)`);
  const match = existingHub.match(regex);
  return match ? `## ${heading}${match[1]}`.trim() : fallback;
}

// 剥离可能存在的 AUTO-GENERATED 注释
let rawHub = existingHub.replace(/^<!--[\s\S]*?-->\s*/, '');

// 提取 Header
let header = '# Coordination Hub — 协调总览\n';
const headerMatch = rawHub.match(/^(# [^\n]+[\s\S]*?)(?=\n## )/);
if (headerMatch) {
  header = headerMatch[1].trim();
}

const waveSection = extractSection('波次记录', '## 波次记录\n\n（待补充）');
const riskSection = extractSection('跨轨风险', '## 跨轨风险\n\n- 无明显跨轨冲突');
const rSection = extractSection('R 阶段清单', '## R 阶段清单\n\n- [ ] 全量回归测试');

// 生成功能总览表
let overviewTable = `## 功能总览表

| Track | 任务 | 状态 | 编码 Commit | 测试 Commit | 合并 Commit |
|-------|------|------|------------|------------|------------|
`;

for (const t of tracks) {
  let statusDisplay = t.phase;
  if (t.unresolvedBugs > 0) {
    statusDisplay += ` (${t.unresolvedBugs} bugs)`;
  }
  overviewTable += `| ${t.track} | ${t.task} | ${statusDisplay} | ${t.codingCommit} | ${t.testCommit} | ${t.mergeCommit} |\n`;
}

// 生成写锁台账
let lockTable = `## 写锁台账

| Track | 写锁代理 | Worktree | Branch | Phase | 最后心跳 |
|-------|----------|----------|--------|-------|----------|
`;

for (const t of tracks) {
  lockTable += `| ${t.track} | ${t.agent} | ${t.worktree} | ${t.branch} | ${t.phase} | ${t.lastHeartbeat} |\n`;
}

const notice = '<!-- AUTO-GENERATED by scripts/aggregate-hub.mjs. DO NOT EDIT DIRECTLY. -->\n';

const finalContent = `${notice}
${header}

${overviewTable.trim()}

${lockTable.trim()}

${waveSection}

${riskSection}

${rSection}
`.trim() + '\n';

if (isCheck) {
  if (existingHub.trim() !== finalContent.trim()) {
    console.error('❌ hub.md 与各轨 progress.md 聚合结果不一致！请运行 node scripts/aggregate-hub.mjs 重新生成。');
    process.exit(1);
  } else {
    console.log('✅ hub.md 与各轨数据一致。');
    process.exit(0);
  }
}

fs.writeFileSync(HUB_FILE, finalContent, 'utf8');
console.log(`✅ 已成功聚合 ${tracks.length} 个 tracks，更新: ${HUB_FILE}`);
