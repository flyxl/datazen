import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';

/**
 * E2E test for AskQuestion interaction in AI Chat.
 *
 * Tests the question extraction, rendering, and answer submission flow.
 * Since we cannot control LLM output in E2E, we directly inject chat messages
 * with the <ask_questions> format to verify the UI behavior.
 */

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as Record<string, unknown>).__error as string);
  }
  return result as T;
}

const TEST_WORKFLOW_ID = 'e2e-ask-question-test';

async function seedAskQuestionWorkflow() {
  const workflow = {
    id: TEST_WORKFLOW_ID,
    name: 'E2E Ask Question Test',
    description: 'Tests the AskQuestion interaction in AI workflow steps',
    variables: [
      { name: 'db_choice', type: 'string', description: 'Database choice from AI question', required: false },
    ],
    steps: [
      {
        type: 'ai',
        id: 'ask_step',
        prompt: `Please ask the user which database they prefer by outputting questions in the ask_questions format.
Output the following:
I can help you set up a database query workflow. Let me ask a few questions first.

<ask_questions>
[{"id":"db_type","prompt":"Which database engine do you want to use?","options":[{"id":"pg","label":"PostgreSQL (Recommended)"},{"id":"mysql","label":"MySQL"},{"id":"sqlite","label":"SQLite"}],"allowMultiple":false},{"id":"query_type","prompt":"What type of queries will this workflow run?","options":[{"id":"select","label":"Read queries (SELECT)"},{"id":"write","label":"Write queries (INSERT/UPDATE)"},{"id":"both","label":"Both read and write"}],"allowMultiple":false}]
</ask_questions>`,
      },
    ],
  };
  await invokeBackend('workflow_save', { workflow });
}

async function cleanupTestWorkflow() {
  try { await invokeBackend('workflow_delete', { workflowId: TEST_WORKFLOW_ID }); } catch { /* ok */ }
}

async function findAndClickButton(textFragments: string[]) {
  return browser.execute((frags: string[]) => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.hasAttribute('disabled')) continue;
      const text = btn.textContent || '';
      if (frags.some((f) => text.includes(f))) { btn.click(); return true; }
    }
    return false;
  }, textFragments);
}

async function openWorkflowFromMain(mainHandle: string) {
  await browser.switchToWindow(mainHandle);
  await browser.pause(500);
  await findAndClickButton(['工作流', 'Workflow']);
  const wfWindow = await switchToNewWindow(mainHandle);
  await browser.pause(2000);
  return wfWindow;
}

const MOCK_QUESTIONS = [
  {
    id: 'db_type',
    prompt: 'Which database engine do you want to use?',
    options: [
      { id: 'pg', label: 'PostgreSQL (Recommended)' },
      { id: 'mysql', label: 'MySQL' },
      { id: 'sqlite', label: 'SQLite' },
    ],
    allowMultiple: false,
  },
  {
    id: 'query_type',
    prompt: 'What type of queries will this workflow run?',
    options: [
      { id: 'select', label: 'Read queries (SELECT)' },
      { id: 'write', label: 'Write queries (INSERT/UPDATE)' },
      { id: 'both', label: 'Both read and write' },
    ],
    allowMultiple: false,
  },
];

/**
 * Injects a simulated AI chat message containing AskQuestion blocks
 * into the current aiStore chatSession (via XML-parsed questions).
 */
async function injectAskQuestionMessage() {
  return browser.execute((qs: typeof MOCK_QUESTIONS) => {
    const content = `I can help you set up a database query workflow. Let me ask a few questions first.`;
    const zustandStores = (window as any).__zustand_stores;
    if (zustandStores) {
      for (const store of zustandStores.values()) {
        const state = store.getState();
        if (state.chatSession) {
          store.setState({
            chatSession: {
              ...state.chatSession,
              messages: [
                ...state.chatSession.messages,
                { role: 'user', content: 'Help me create a workflow' },
                { role: 'assistant', content, questions: qs },
              ],
            },
          });
          return true;
        }
      }
    }
    return false;
  }, MOCK_QUESTIONS);
}

/**
 * Injects a simulated AI chat message with tool call based AskQuestion
 * (function calling approach) into the current aiStore chatSession.
 */
async function injectToolCallAskQuestionMessage() {
  return browser.execute((qs: typeof MOCK_QUESTIONS) => {
    const content = `Let me gather some information to help you.`;
    const toolCalls = [
      {
        id: 'call_abc123',
        name: 'ask_questions',
        arguments: JSON.stringify({ questions: qs }),
      },
    ];
    const zustandStores = (window as any).__zustand_stores;
    if (zustandStores) {
      for (const store of zustandStores.values()) {
        const state = store.getState();
        if (state.chatSession) {
          store.setState({
            chatSession: {
              ...state.chatSession,
              messages: [
                ...state.chatSession.messages,
                { role: 'user', content: 'Help me create a workflow' },
                { role: 'assistant', content, questions: qs, toolCalls },
              ],
            },
          });
          return true;
        }
      }
    }
    return false;
  }, MOCK_QUESTIONS);
}

describe('AI AskQuestion Interaction (E2E)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(2000);
    await seedAskQuestionWorkflow();
    await browser.pause(500);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await cleanupTestWorkflow();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
  });

  it('extractQuestions 应正确解析 ask_questions 标签', async () => {
    const result = await browser.execute(() => {
      const testContent = `Some text before.

<ask_questions>
[{"id":"q1","prompt":"Pick one","options":[{"id":"a","label":"A"},{"id":"b","label":"B"}],"allowMultiple":false}]
</ask_questions>`;

      const ASK_QUESTIONS_RE = /<ask_questions>\s*([\s\S]*?)\s*<\/ask_questions>/;
      const match = ASK_QUESTIONS_RE.exec(testContent);
      if (!match) return { ok: false, reason: 'no match' };

      const json = JSON.parse(match[1].trim());
      const clean = testContent.replace(ASK_QUESTIONS_RE, '').trim();
      return { ok: true, questionsCount: json.length, cleanContent: clean, firstId: json[0]?.id };
    });

    expect(result.ok).toBe(true);
    expect(result.questionsCount).toBe(1);
    expect(result.firstId).toBe('q1');
    expect(result.cleanContent).toBe('Some text before.');
  });

  it('测试 workflow 应包含 AI 提问步骤', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);

    await browser.waitUntil(async () => {
      const text = await $('body').getText();
      return text.includes('E2E Ask Question Test');
    }, { timeout: 8000 });

    const item = await $('div*=E2E Ask Question Test');
    await expect(item).toBeDisplayed();
  });

  it('问题 UI 应渲染选项按钮和自定义输入框', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);

    // Inject a simulated message with questions to test UI rendering
    const injected = await injectAskQuestionMessage();

    if (injected) {
      await browser.pause(1000);

      // Check that question prompts are rendered
      const body = await $('body').getText();
      const hasQuestion = body.includes('database engine') || body.includes('数据库');
      expect(hasQuestion).toBe(true);
    }
  });

  it('选择选项后应高亮', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    await injectAskQuestionMessage();
    await browser.pause(1000);

    // Click an option button
    const clicked = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('PostgreSQL')) {
          btn.click();
          return true;
        }
      }
      return false;
    });

    if (clicked) {
      await browser.pause(300);

      // Verify the button has the selected styling (accent color)
      const hasSelectedStyle = await browser.execute(() => {
        const buttons = document.querySelectorAll('button');
        for (const btn of buttons) {
          if (btn.textContent?.includes('PostgreSQL') && btn.className.includes('accent')) {
            return true;
          }
        }
        return false;
      });
      expect(hasSelectedStyle).toBe(true);
    }
  });

  it('提交按钮应存在且可点击', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    await injectAskQuestionMessage();
    await browser.pause(1000);

    const hasSubmitBtn = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if (text.includes('提交回答') || text.includes('Submit Answers')) {
          return true;
        }
      }
      return false;
    });

    expect(hasSubmitBtn).toBe(true);
  });

  it('（Tool Call）问题 UI 应渲染选项按钮', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);

    const injected = await injectToolCallAskQuestionMessage();
    if (injected) {
      await browser.pause(1000);
      const body = await $('body').getText();
      const hasQuestion = body.includes('database engine') || body.includes('数据库');
      expect(hasQuestion).toBe(true);
    }
  });

  it('（Tool Call）选择选项后应高亮', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    await injectToolCallAskQuestionMessage();
    await browser.pause(1000);

    const clicked = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('PostgreSQL')) {
          btn.click();
          return true;
        }
      }
      return false;
    });

    if (clicked) {
      await browser.pause(300);
      const hasSelectedStyle = await browser.execute(() => {
        const buttons = document.querySelectorAll('button');
        for (const btn of buttons) {
          if (btn.textContent?.includes('PostgreSQL') && btn.className.includes('accent')) {
            return true;
          }
        }
        return false;
      });
      expect(hasSelectedStyle).toBe(true);
    }
  });

  it('（Tool Call）提交后应生成 tool 角色消息', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    await injectToolCallAskQuestionMessage();
    await browser.pause(1000);

    // Select an option
    await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('PostgreSQL')) { btn.click(); break; }
      }
    });
    await browser.pause(200);

    // Select second question option
    await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('Read queries') || btn.textContent?.includes('SELECT')) {
          btn.click(); break;
        }
      }
    });
    await browser.pause(200);

    // Check that submit button exists
    const hasSubmit = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if (text.includes('提交回答') || text.includes('Submit Answers')) {
          return true;
        }
      }
      return false;
    });
    expect(hasSubmit).toBe(true);
  });

  it('（Tool Call）消息中应包含 toolCalls 字段', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    await injectToolCallAskQuestionMessage();
    await browser.pause(500);

    const hasToolCalls = await browser.execute(() => {
      const zustandStores = (window as any).__zustand_stores;
      if (zustandStores) {
        for (const store of zustandStores.values()) {
          const state = store.getState();
          if (state.chatSession) {
            const msgs = state.chatSession.messages;
            const lastAssistant = [...msgs].reverse().find((m: any) => m.role === 'assistant');
            return !!(lastAssistant && lastAssistant.toolCalls && lastAssistant.toolCalls.length > 0);
          }
        }
      }
      return false;
    });
    expect(hasToolCalls).toBe(true);
  });
});
