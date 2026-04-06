import { writable } from 'svelte/store';
import type { ChatMessage, ToolCallRecord } from '../types';

function generateId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function createChatStore() {
  const { subscribe, set, update } = writable<ChatMessage[]>([]);

  return {
    subscribe,

    addUserMessage(content: string) {
      const msg: ChatMessage = {
        id: generateId(),
        role: 'user',
        content,
        timestamp: new Date().toISOString(),
      };
      update((msgs) => [...msgs, msg]);
    },

    addAssistantMessage(
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
    },

    appendToLast(delta: string) {
      update((msgs) => {
        if (msgs.length === 0) return msgs;
        const last = msgs[msgs.length - 1];
        const updated: ChatMessage = { ...last, content: last.content + delta };
        return [...msgs.slice(0, -1), updated];
      });
    },

    startStreaming() {
      const msg: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        content: '',
        timestamp: new Date().toISOString(),
      };
      update((msgs) => [...msgs, msg]);
      isStreaming.set(true);
    },

    stopStreaming() {
      isStreaming.set(false);
    },

    clear() {
      set([]);
      isStreaming.set(false);
    },
  };
}

export const isStreaming = writable<boolean>(false);
export const chat = createChatStore();
