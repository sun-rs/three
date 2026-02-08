# Roundtable 提示词改进说明

## 改进概述

本次改进参考了 llm-council 项目的提示词工程最佳实践，显著增强了多轮讨论的智能性和有效性。

## 主要改进

### 1. **增强的 Round 1 提示词**

**改进前**：
```
Reply with:
1) Position (1-2 sentences)
2) Key reasons (bullets)
3) Risks (bullets)
4) Recommendation (1 sentence)
```

**改进后**：
- ✅ 明确的任务说明："This is the first round of a multi-round deliberative discussion"
- ✅ 结构化的输出要求（POSITION, KEY REASONS, POTENTIAL RISKS, RECOMMENDATION）
- ✅ 具体的格式指导（使用 bullet points，引用证据）
- ✅ 上下文说明："Your response will be shared with other participants who will critique and build upon it"

### 2. **大幅增强的 Round 2+ 提示词**

**改进前**（太简洁）：
```
Reply with:
1) Do you change your position?
2) Agreements with peers (bullets)
3) Remaining disagreement (bullets)
4) Updated recommendation
```

**改进后**（详细且强制互动）：
```
CRITICAL INSTRUCTIONS: This is a DELIBERATIVE DISCUSSION, not a monologue.
You MUST engage with the responses above.

Your response MUST include ALL of the following sections:

1. POSITION UPDATE:
   - Have you changed your position? (Yes/No/Partially)
   - If yes/partially: explain what changed your mind and why
   - If no: explain why you maintain your position despite peer arguments

2. AGREEMENTS WITH [PEERS]:
   - List specific points where you agree
   - For EACH agreement, you MUST:
     * Cite which response you're agreeing with
     * Explain WHY you find this point compelling
     * Add supporting evidence or reasoning

3. DISAGREEMENTS WITH [PEERS]:
   - List specific points where you disagree
   - For EACH disagreement, you MUST:
     * Cite which response you're disagreeing with
     * Explain WHY you disagree (counter-arguments/evidence)
     * Address the strongest version of their argument

4. NEW INSIGHTS & SYNTHESIS:
   - What new insights emerged?
   - Are there middle-ground positions?
   - What questions remain unresolved?
   - Has the discussion revealed blind spots?

5. UPDATED RECOMMENDATION:
   - Current recommendation based on full discussion
   - How it differs from Round 1
   - Confidence level (high/medium/low)

CRITICAL REQUIREMENTS:
- Be SPECIFIC when referencing peers
- ENGAGE with the strongest arguments against your position
- Show that you're listening and thinking, not just repeating
```

**关键改进点**：
- ✅ 强制要求引用具体的回应（"I agree with Response A's point about..."）
- ✅ 要求解释"为什么"同意或不同意
- ✅ 要求处理最强的反对论点（steel-man principle）
- ✅ 要求展示思维过程的演变
- ✅ 明确这是"讨论"而非"独白"

### 3. **结构化的上下文构建**

**改进前**：
```
Participants: 3
Success: 3
Failed: 0

[Response A]
<text>

[Response B]
<text>
```

**改进后**：
```
ROUND SUMMARY:
- Total participants: 3
- Successful responses: 3
- Failed responses: 0

INSTRUCTIONS FOR READING THESE RESPONSES:
1. Read ALL responses carefully and completely
2. Identify points of AGREEMENT (where multiple responses align)
3. Identify points of DISAGREEMENT (where responses conflict)
4. Evaluate the STRENGTH of each argument (evidence, logic, reasoning)
5. Consider what's MISSING from the discussion
6. Think about how to SYNTHESIZE or RESOLVE disagreements

YOUR TASK: Respond to these viewpoints by engaging with specific arguments,
not by repeating your previous position.

━━━ Response A ━━━
<text>

━━━ Response B ━━━
<text>

━━━ END OF RESPONSES ━━━

ANALYSIS CHECKLIST (consider before responding):
□ Which responses share common ground?
□ What are the key points of disagreement?
□ Which arguments are backed by evidence vs. opinion?
□ Are there any logical fallacies or weak reasoning?
□ What perspectives or considerations are missing?
□ Is there a synthesis position that addresses multiple viewpoints?
```

**关键改进点**：
- ✅ 明确的阅读指导
- ✅ 分析检查清单
- ✅ 视觉分隔符（━━━）使内容更清晰
- ✅ 强调任务是"回应"而非"重复"

### 4. **新增：讨论动态分析**

新增 `analyzeDiscussionDynamics()` 函数，自动检测：
- **收敛度**：参与者的观点是否趋于一致
- **相似度得分**：跨轮次的文本相似度
- **活跃分歧**：是否还有实质性的不同观点
- **建议**：是否应该继续讨论或提前结束

```javascript
{
  converged: false,
  convergence_score: 0.45,
  convergence_ratio: 0.33,
  avg_similarity: 0.45,
  active_disagreement: true,
  recommendation: 'continue',
  reason: 'discussion_still_evolving',
  agent_similarities: [...]
}
```

**自动早停机制**：
- 如果检测到讨论已收敛（相似度 > 75% 或重复率 > 65%）
- 自动提前结束，避免无意义的重复

### 5. **新增：主席综合功能**

新增 `synthesizeRoundtableDiscussion()` 函数，类似 llm-council 的 Stage 3：

```javascript
await synthesizeRoundtableDiscussion({
  client,
  directory,
  parentSessionID,
  parentKey,
  store,
  catalog,
  topic,
  roundsOut,
  chairman: 'claude-sonnet',  // 指定主席 agent
  model: 'anthropic/claude-sonnet-4.5',
});
```

**主席报告包含**：
1. **执行摘要**：核心问题和最终建议
2. **一致点**：参与者达成共识的要点
3. **分歧点**：不同观点及其理由
4. **讨论演变**：观点如何跨轮次变化
5. **最终综合**：整合所有视角的建议
6. **信心评估**：对综合结果的信心水平

## 使用示例

### 基本用法（使用改进的提示词）

```javascript
const result = await runRoundtable({
  client,
  directory,
  worktree,
  parentSessionID,
  conversationID,
  topic: "Should we adopt microservices architecture for our new project?",
  participants: [
    { agent: 'architect', name: 'System Architect' },
    { agent: 'devops', name: 'DevOps Engineer' },
    { agent: 'developer', name: 'Senior Developer' },
  ],
  rounds: 3,
  round1ForceNew: true,
  roundContextLevel: 'rich',
  roundAnonymousViewpoints: false,  // 显示参与者名字
  persistRoundArtifacts: true,
});

// 检查讨论动态
for (const round of result.rounds) {
  if (round.discussion_dynamics) {
    console.log(`Round ${round.round}:`, round.discussion_dynamics);
  }
}

// 结果会返回给主 agent（如 sisyphus）
// 主 agent 会自己分析和综合这些讨论内容
```

### 主 Agent 如何使用讨论结果

当主 agent（如 sisyphus）调用 `/roundtable` 后，会收到结构化的讨论结果：

```javascript
{
  success: true,
  operation: 'roundtable',
  topic: "Should we adopt microservices...",
  rounds: [
    {
      round: 1,
      contributions: [
        { agent: 'architect', text: '...', success: true },
        { agent: 'devops', text: '...', success: true },
        { agent: 'developer', text: '...', success: true },
      ],
      discussion_dynamics: null  // Round 1 没有动态分析
    },
    {
      round: 2,
      contributions: [...],
      discussion_dynamics: {
        converged: false,
        convergence_score: 0.45,
        recommendation: 'continue',
        reason: 'discussion_still_evolving'
      }
    },
    // ...
  ],
  aborted_reason: null,  // 或 'discussion_converged_at_round_3'
}
```

**主 agent 会自动**：
1. 读取所有轮次的讨论内容
2. 分析一致点和分歧点
3. 考虑讨论动态（是否收敛）
4. 生成自己的综合报告给用户

**不需要额外的"主席综合"函数**，因为主 agent 本身就是"主席"！

### 匿名讨论（减少偏见）

```javascript
const result = await runRoundtable({
  // ...
  roundAnonymousViewpoints: true,  // 使用 "Response A", "Response B" 而非名字
  // 这样可以减少基于角色的偏见
});
```

## 预期效果

### 改进前的典型问题：
- ❌ 参与者只是重复自己的观点
- ❌ 没有真正的互动和辩论
- ❌ 讨论不收敛，每轮都说相似的话
- ❌ 缺少对分歧的深入分析

### 改进后的预期效果：
- ✅ 参与者会引用和回应彼此的具体论点
- ✅ 观点会随着讨论演变（改变立场或加强论证）
- ✅ 自动检测收敛，避免无意义的重复
- ✅ 主席综合提供清晰的最终结论
- ✅ 更像真实的专家圆桌讨论

## 配置建议

### 对于深度技术讨论：
```javascript
{
  rounds: 3-4,
  roundContextLevel: 'rich',  // 6000 chars per agent
  roundAnonymousViewpoints: false,  // 显示专家身份
  roundStageTimeoutSecs: 120,  // 给足够时间思考
}
```

### 对于快速决策：
```javascript
{
  rounds: 2,
  roundContextLevel: 'compact',  // 1400 chars per agent
  roundAnonymousViewpoints: true,  // 减少偏见
  roundStageTimeoutSecs: 60,
}
```

### 对于争议性话题：
```javascript
{
  rounds: 4-5,  // 需要更多轮次达成共识
  roundContextLevel: 'rich',
  roundAnonymousViewpoints: true,  // 匿名减少立场固化
  round2OnlyStage1Success: true,  // 只让成功的参与者继续
}
```

## 技术细节

### 文本相似度计算

使用 Jaccard 相似度算法：
```javascript
similarity = |intersection(words1, words2)| / |union(words1, words2)|
```

- 过滤掉长度 ≤ 2 的短词
- 忽略标点和大小写
- 返回 0-1 之间的分数

### 收敛检测阈值

- **高相似度**：> 80% 相似度
- **收敛比例**：> 65% 的参与者达到高相似度
- **或**：平均相似度 > 75% 且回复长度减少 > 200 字符

### 早停条件

满足以下所有条件时触发早停：
1. 至少完成 2 轮讨论
2. 收敛检测返回 `converged: true`
3. 建议为 `recommendation: 'stop'`

## 调试和监控

### 查看讨论动态

```javascript
const dynamics = analyzeDiscussionDynamics(roundsOut);
console.log('Converged:', dynamics.converged);
console.log('Similarity:', dynamics.avg_similarity);
console.log('Recommendation:', dynamics.recommendation);
console.log('Agent similarities:', dynamics.agent_similarities);
```

### 查看持久化的 artifacts

```bash
ls -la .roundtable/roundtable-artifacts/<session-hash>/<run-id>/
# run.start.json - 运行配置
# round-01.json - 第1轮结果（含 discussion_dynamics）
# round-02.json - 第2轮结果
# run.complete.json - 完整结果
```

## 与 llm-council 的对比

| 特性 | llm-council | Roundtable (改进后) |
|------|-------------|---------------------|
| 固定流程 | 3阶段（回答→评价→综合） | 灵活N轮讨论 |
| 提示词质量 | ⭐⭐⭐⭐⭐ 非常详细 | ⭐⭐⭐⭐⭐ 现在同样详细 |
| 强制互动 | ✅ 必须评价和排名 | ✅ 必须引用和回应 |
| 收敛检测 | ❌ 无 | ✅ 自动检测 |
| 早停机制 | ❌ 无 | ✅ 自动早停 |
| 主席综合 | ✅ Stage 3（独立阶段） | ✅ 主 agent 自己综合 |
| 会话持久化 | ❌ 无 | ✅ 跨轮次复用会话 |
| 灵活性 | ⭐⭐ 固定3阶段 | ⭐⭐⭐⭐⭐ 高度可配置 |

**关键区别**：
- llm-council 的 Stage 3 是一个**独立的 LLM 调用**，专门用于综合
- Roundtable 的综合是由**主 agent（调用者）自己完成**，更自然

**为什么 Roundtable 的方式更好**：
1. ✅ 主 agent 已经有完整的上下文
2. ✅ 不需要额外的 API 调用
3. ✅ 主 agent 可以根据用户的具体问题定制综合方式
4. ✅ 更符合"工具调用"的语义（工具返回数据，agent 处理数据）

## 后续改进建议

1. **添加投票机制**：让参与者对最终建议投票
2. **分歧可视化**：生成分歧点的结构化摘要
3. **论点追踪**：追踪特定论点在多轮中的演变
4. **自适应轮次**：根据收敛速度动态调整轮数
5. **子话题分支**：当出现多个分歧点时，分别讨论

## 总结

本次改进的核心是**从"简洁提示"转向"明确指令"**：

- ✅ 告诉 AI **为什么**要做某事
- ✅ 告诉 AI **如何**做（具体格式和要求）
- ✅ 提供**检查清单**和**示例**
- ✅ 强制**互动**而非独白
- ✅ 自动**检测**讨论质量

这些改进使 Roundtable 从"多个 AI 各说各话"变成了"真正的多轮辩论和综合"。
