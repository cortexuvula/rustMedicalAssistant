import { writable, get } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import * as chatApi from '../api/chat';
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

    async sendMessage(content: string) {
      // Add user message
      this.addUserMessage(content);
      this.startStreaming();

      // Set up event listeners for streaming
      let tokenUnlisten: UnlistenFn | null = null;
      let doneUnlisten: UnlistenFn | null = null;
      let errorUnlisten: UnlistenFn | null = null;

      const cleanup = () => {
        tokenUnlisten?.();
        doneUnlisten?.();
        errorUnlisten?.();
        this.stopStreaming();
      };

      try {
        tokenUnlisten = await listen<string>('chat-token', (event) => {
          this.appendToLast(event.payload);
        });
        doneUnlisten = await listen('chat-done', () => {
          cleanup();
        });
        errorUnlisten = await listen<string>('chat-error', (event) => {
          this.appendToLast(`\n\nError: ${event.payload}`);
          cleanup();
        });

        // Build messages for the API (convert store format to API format)
        const currentMessages = get({ subscribe });
        const apiMessages = currentMessages
          .filter(
            (m) =>
              m.role === 'user' || (m.role === 'assistant' && m.content)
          )
          .slice(0, -1) // exclude the empty streaming message we just added
          .map((m) => ({ role: m.role, content: m.content }));

        await chatApi.chatStream(apiMessages);
      } catch (e: any) {
        this.appendToLast(`\n\nError: ${e?.toString() || 'Chat failed'}`);
        cleanup();
      }
    },

    clear() {
      set([]);
      isStreaming.set(false);
    },
  };
}

export const isStreaming = writable<boolean>(false);
export const chat = createChatStore();
