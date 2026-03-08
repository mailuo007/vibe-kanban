import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  CreateFeishuBotRequest,
  SendFeishuMessageInput,
  UpdateFeishuBotRequest,
} from 'shared/types';
import { feishuApi } from '@/shared/lib/api';
import { workspaceFeishuKeys } from '@/shared/hooks/useWorkspaceFeishuBindings';

export const feishuKeys = {
  all: ['feishu'] as const,
  bots: () => [...feishuKeys.all, 'bots'] as const,
  bot: (botId: string) => [...feishuKeys.bots(), botId] as const,
  targets: (botId: string) => [...feishuKeys.bot(botId), 'targets'] as const,
};

export function useFeishuBots() {
  return useQuery({
    queryKey: feishuKeys.bots(),
    queryFn: () => feishuApi.listBots(),
  });
}

export function useFeishuTargets(botId: string | undefined) {
  return useQuery({
    queryKey: botId ? feishuKeys.targets(botId) : feishuKeys.targets('unknown'),
    queryFn: () => feishuApi.listTargets(botId!),
    enabled: !!botId,
  });
}

export function useCreateFeishuBot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: CreateFeishuBotRequest) => feishuApi.createBot(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: feishuKeys.bots() });
      queryClient.invalidateQueries({ queryKey: workspaceFeishuKeys.all });
    },
  });
}

export function useUpdateFeishuBot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      botId,
      data,
    }: {
      botId: string;
      data: UpdateFeishuBotRequest;
    }) => feishuApi.updateBot(botId, data),
    onSuccess: (bot) => {
      queryClient.invalidateQueries({ queryKey: feishuKeys.bots() });
      queryClient.invalidateQueries({ queryKey: feishuKeys.bot(bot.id) });
      queryClient.invalidateQueries({ queryKey: workspaceFeishuKeys.all });
    },
  });
}

export function useDeleteFeishuBot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (botId: string) => feishuApi.deleteBot(botId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: feishuKeys.bots() });
      queryClient.invalidateQueries({ queryKey: workspaceFeishuKeys.all });
    },
  });
}

export function useValidateFeishuBot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (botId: string) => feishuApi.validateBot(botId),
    onSuccess: (_result, botId) => {
      queryClient.invalidateQueries({ queryKey: feishuKeys.bots() });
      queryClient.invalidateQueries({ queryKey: feishuKeys.targets(botId) });
      queryClient.invalidateQueries({ queryKey: workspaceFeishuKeys.all });
    },
  });
}

export function useDiscoverFeishuTargets() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (botId: string) => feishuApi.discoverTargets(botId),
    onSuccess: (_targets, botId) => {
      queryClient.invalidateQueries({ queryKey: feishuKeys.targets(botId) });
      queryClient.invalidateQueries({ queryKey: feishuKeys.bots() });
      queryClient.invalidateQueries({ queryKey: workspaceFeishuKeys.all });
    },
  });
}

export function useSendFeishuTestMessage() {
  return useMutation({
    mutationFn: ({
      botId,
      targetId,
      data,
    }: {
      botId: string;
      targetId: string;
      data: SendFeishuMessageInput;
    }) => feishuApi.sendTestMessage(botId, targetId, data),
  });
}
