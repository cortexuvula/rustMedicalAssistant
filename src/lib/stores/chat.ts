import { writable, get } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import * as chatApi from '../api/chat';
import type { ChatMessage, ToolCallRecord } from '../types';

function generateId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export const isStreaming = writable<boolean>(false);

function createChatStore() {
  const store = writable<ChatMessage[]>([]);
  const { subscribe, set, update } = store;

  function addUserMessage(content: string) {
    const msg: ChatMessage = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
    };
    update((msgs) => [...msgs, msg]);
  }

  function addAssistantMessage(
    content: string,
    agent?: string,
    tool_calls?: ToolCallRecord[]
  ) {
    const msg: ChatMessage = {
      id: generateId(),
      role: 'assistant',
      content,
      timestamp: new Date().toISOString(),
      agent,
      tool_calls,
    };
    update((msgs) => [...msgs, msg]);
  }

  function appendToLast(delta: string) {
    update((msgs) => {
      if (msgs.length === 0) return msgs;
      const last = msgs[msgs.length - 1];
      const updated: ChatMessage = { ...last, content: last.content + delta };
      return [...msgs.slice(0, -1), updated];
    });
  }

  function startStreaming() {
    const msg: ChatMessage = {
      id: generateId(),
      role: 'assistant',
      content: '',
      timestamp: new Date().toISOString(),
    };
    update((msgs) => [...msgs, msg]);
    isStreaming.set(true);
  }

  function stopStreaming() {
    isStreaming.set(false);
  }

  async function sendMessage(content: string) {
    addUserMessage(content);
    startStreaming();

    let tokenUnlisten: UnlistenFn | null = null;
    let doneUnlisten: UnlistenFn | null = null;
    let errorUnlisten: UnlistenFn | null = null;
    let cleaned = false;

    const cleanup = () => {
      if (cleaned) return;
      cleaned = true;
      tokenUnlisten?.();
      doneUnlisten?.();
      errorUnlisten?.();
      stopStreaming();
    };

    try {
      tokenUnlisten = await listen<string>('chat-token', (event) => {
        appendToLast(event.payload);
      });
      doneUnlisten = await listen('chat-done', () => {
        cleanup();
      });
      errorUnlisten = await listen<string>('chat-error', (event) => {
        appendToLast(`\n\nError: ${event.payload}`);
        cleanup();
      });

      // Build messages for the API — use get(store) to read current value
      const currentMessages = get(store);
      const apiMessages = currentMessages
        .filter(
          (m) =>
            m.role === 'user' || (m.role === 'assistant' && m.content)
        )
        .slice(0, -1) // exclude the empty streaming message we just added
        .map((m) => ({ role: m.role, content: m.content }));

      await chatApi.chatStream(apiMessages);
    } catch (e: any) {
      appendToLast(`\n\nError: ${e?.toString() || 'Chat failed'}`);
      cleanup();
    }
  }

  function clear() {
    set([]);
    isStreaming.set(false);
  }

  return {
    subscribe,
    addUserMessage,
    addAssistantMessage,
    appendToLast,
    startStreaming,
    stopStreaming,
    sendMessage,
    clear,
  };
}

export const chat = createChatStore();
