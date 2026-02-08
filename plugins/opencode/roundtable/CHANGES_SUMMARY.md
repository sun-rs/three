# 提示词改进总结

## 改进内容

### 1. ✅ 增强 Round 1 提示词
- 添加明确的任务说明
- 结构化输出要求（POSITION, KEY REASONS, RISKS, RECOMMENDATION）
- 具体的格式指导

### 2. ✅ 大幅增强 Round 2+ 提示词
- **强制互动**：必须引用具体的回应（"I agree with Response A's point about..."）
- **强制解释**：必须说明为什么同意/不同意
- **强制回应**：必须处理最强的反对论点
- **展示演变**：必须说明观点如何变化
- 5个必填部分：POSITION UPDATE, AGREEMENTS, DISAGREEMENTS, NEW INSIGHTS, UPDATED RECOMMENDATION

### 3. ✅ 结构化上下文构建
- 添加阅读指导
- 添加分析检查清单
- 使用视觉分隔符（━━━）
- 强调任务是"回应"而非"重复"

### 4. ✅ 自动收敛检测
- 新增 `analyzeDiscussionDynamics()` 函数
- 计算跨轮次的文本相似度
- 检测讨论是否收敛或仍在演变
- 自动早停机制（避免无意义重复）

### 5. ✅ 讨论动态记录
- 每轮后自动分析讨论动态
- 记录收敛度、相似度、建议
- 持久化到 artifact 文件

## 核心改进原则

从 **"简洁提示"** 转向 **"明确指令"**：

- ✅ 告诉 AI **为什么**要做某事
- ✅ 告诉 AI **如何**做（具体格式和要求）
- ✅ 提供**检查清单**
- ✅ 强制**互动**而非独白
- ✅ 自动**检测**讨论质量

## 预期效果

### 改进前：
- ❌ 参与者只是重复自己的观点
- ❌ 没有真正的互动和辩论
- ❌ 讨论不收敛

### 改进后：
- ✅ 参与者会引用和回应彼此的具体论点
- ✅ 观点会随着讨论演变
- ✅ 自动检测收敛，避免无意义重复
- ✅ 更像真实的专家圆桌讨论

## 文件变更

- `src/orchestrator.mjs` - 修改了 3 个函数，新增了 2 个函数：
  - ✏️ `buildRoundPrompt()` - 大幅增强提示词
  - ✏️ `buildRoundContext()` - 结构化上下文
  - ✏️ `runRoundtable()` - 集成收敛检测和早停
  - ➕ `analyzeDiscussionDynamics()` - 新增收敛检测
  - ➕ `calculateTextSimilarity()` - 新增相似度计算

## 使用方式

**无需改变调用方式**！只需正常使用 `/roundtable` 命令：

```javascript
const result = await runRoundtable({
  topic: "Should we adopt microservices?",
  participants: ['architect', 'devops', 'developer'],
  rounds: 3,
  // ... 其他配置
});

// 主 agent 会收到结构化的讨论结果
// 主 agent 自己分析和综合，无需额外函数
```

## 与 llm-council 的关键区别

| 维度 | llm-council | Roundtable |
|------|-------------|-----------|
| 综合方式 | Stage 3 独立调用 LLM | 主 agent 自己综合 |
| 优势 | 专门的综合阶段 | 更自然，无需额外调用 |

**Roundtable 的方式更好**，因为：
1. 主 agent 已经有完整上下文
2. 不需要额外的 API 调用
3. 主 agent 可以根据用户问题定制综合方式
4. 更符合"工具调用"的语义

## 测试建议

1. 运行一个 3 轮讨论，观察：
   - Round 2 的回应是否引用了 Round 1 的具体论点
   - Round 3 是否展示了观点的演变
   - 是否触发了早停机制

2. 查看 artifact 文件：
   ```bash
   cat .roundtable/roundtable-artifacts/<hash>/<run-id>/round-02.json
   ```
   检查 `discussion_dynamics` 字段

3. 对比改进前后的讨论质量

## 后续可能的改进

1. 添加投票机制
2. 分歧可视化
3. 论点追踪
4. 自适应轮次
5. 子话题分支

---

详细文档见 `PROMPT_IMPROVEMENTS.md`
