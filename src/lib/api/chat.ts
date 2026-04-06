import { invoke } from '@tauri-apps/api/core';

export interface ChatMessageInput {
  role: string;
  content: string;
}

export async function chatSend(
  messages: ChatMessageInput[],
  model?: string,
  systemPrompt?: string
): Promise<string> {
  return invoke('chat_send', { messages, model, systemPrompt });
}

export async function chatStream(
  messages: ChatMessageInput[],
  model?: string,
  systemPrompt?: string
): Promise<void> {
  return invoke('chat_stream', { messages, model, systemPrompt });
}

export async function chatWithAgent(
  message: string,
  agentName: string,
  conversationHistory?: ChatMessageInput[]
): Promise<any> {
  return invoke('chat_with_agent', { message, agentName, conversationHistory });
}

export async function listAiProviders(): Promise<string[]> {
  return invoke('list_ai_providers');
}

export async function setActiveProvider(name: string): Promise<boolean> {
  return invoke('set_active_provider', { name });
}

export async function reinitProviders(): Promise<string[]> {
  return invoke('reinit_providers');
}
