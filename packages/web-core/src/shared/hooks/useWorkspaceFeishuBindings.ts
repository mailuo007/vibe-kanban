import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  CreateWorkspaceFeishuBindingRequest,
  SendWorkspaceFeishuTestMessageRequest,
  UpdateWorkspaceFeishuBindingRequest,
} from 'shared/types';
import { attemptsApi } from '@/shared/lib/api';

export const workspaceFeishuKeys = {
  all: ['workspace-feishu'] as const,
  bindings: (workspaceId: string) =>
    [...workspaceFeishuKeys.all, workspaceId, 'bindings'] as const,
  bindableTargets: (workspaceId: string) =>
    [...workspaceFeishuKeys.all, workspaceId, 'bindable-targets'] as const,
};

export function useWorkspaceFeishuBindings(workspaceId: string | undefined) {
  return useQuery({
    queryKey: workspaceId
      ? workspaceFeishuKeys.bindings(workspaceId)
      : workspaceFeishuKeys.bindings('unknown'),
    queryFn: () => attemptsApi.listFeishuBindings(workspaceId!),
    enabled: !!workspaceId,
  });
}

export function useWorkspaceFeishuBindableTargets(
  workspaceId: string | undefined
) {
  return useQuery({
    queryKey: workspaceId
      ? workspaceFeishuKeys.bindableTargets(workspaceId)
      : workspaceFeishuKeys.bindableTargets('unknown'),
    queryFn: () => attemptsApi.listFeishuBindableTargets(workspaceId!),
    enabled: !!workspaceId,
  });
}

export function useCreateWorkspaceFeishuBinding(workspaceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: CreateWorkspaceFeishuBindingRequest) =>
      attemptsApi.createFeishuBinding(workspaceId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: workspaceFeishuKeys.bindings(workspaceId),
      });
    },
  });
}

export function useUpdateWorkspaceFeishuBinding(workspaceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      bindingId,
      data,
    }: {
      bindingId: string;
      data: UpdateWorkspaceFeishuBindingRequest;
    }) => attemptsApi.updateFeishuBinding(workspaceId, bindingId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: workspaceFeishuKeys.bindings(workspaceId),
      });
    },
  });
}

export function useDeleteWorkspaceFeishuBinding(workspaceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (bindingId: string) =>
      attemptsApi.deleteFeishuBinding(workspaceId, bindingId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: workspaceFeishuKeys.bindings(workspaceId),
      });
    },
  });
}

export function useSendWorkspaceFeishuTestMessage(workspaceId: string) {
  return useMutation({
    mutationFn: (data: SendWorkspaceFeishuTestMessageRequest) =>
      attemptsApi.sendFeishuTestMessage(workspaceId, data),
  });
}
