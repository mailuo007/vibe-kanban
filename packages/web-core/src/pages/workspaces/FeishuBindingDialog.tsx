import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { create, useModal } from '@ebay/nice-modal-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@vibe/ui/components/KeyboardDialog';
import { Alert, AlertDescription } from '@vibe/ui/components/Alert';
import { Button } from '@vibe/ui/components/Button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@vibe/ui/components/Select';
import { Switch } from '@vibe/ui/components/Switch';
import type {
  UpdateWorkspaceFeishuBindingRequest,
  WorkspaceFeishuBinding,
} from 'shared/types';
import { defineModal } from '@/shared/lib/modals';
import {
  useCreateWorkspaceFeishuBinding,
  useDeleteWorkspaceFeishuBinding,
  useSendWorkspaceFeishuTestMessage,
  useUpdateWorkspaceFeishuBinding,
  useWorkspaceFeishuBindableTargets,
  useWorkspaceFeishuBindings,
} from '@/shared/hooks/useWorkspaceFeishuBindings';

export interface FeishuBindingDialogProps {
  workspaceId: string;
}

const ACK_REACTION_EMOJI_OPTIONS = [
  'Typing',
  'OnIt',
  'OK',
  'THUMBSUP',
  'CheckMark',
] as const;

const FeishuBindingDialogImpl = create<FeishuBindingDialogProps>(
  ({ workspaceId }) => {
    const modal = useModal();
    const { t } = useTranslation('common');
    const {
      data: bindingsData,
      error: bindingsError,
      isLoading: bindingsLoading,
      refetch: refetchBindings,
    } = useWorkspaceFeishuBindings(workspaceId);
    const {
      data: bindableTargetsData,
      error: targetsError,
      isLoading: targetsLoading,
      refetch: refetchTargets,
    } = useWorkspaceFeishuBindableTargets(workspaceId);
    const createBinding = useCreateWorkspaceFeishuBinding(workspaceId);
    const updateBinding = useUpdateWorkspaceFeishuBinding(workspaceId);
    const deleteBinding = useDeleteWorkspaceFeishuBinding(workspaceId);
    const sendTestMessage = useSendWorkspaceFeishuTestMessage(workspaceId);
    const bindings = bindingsData ?? [];
    const bindableTargets = bindableTargetsData ?? [];

    useEffect(() => {
      if (!modal.visible) return;

      void refetchBindings();
      void refetchTargets();
    }, [modal.visible, workspaceId, refetchBindings, refetchTargets]);

    const targetLookup = useMemo(() => {
      const map = new Map<
        string,
        { botId: string; botName: string; targetName: string }
      >();

      for (const option of bindableTargets) {
        for (const target of option.targets) {
          map.set(target.id, {
            botId: option.bot.id,
            botName: option.bot.name,
            targetName: target.name,
          });
        }
      }

      return map;
    }, [bindableTargets]);

    const availableTargets = useMemo(
      () =>
        bindableTargets.flatMap((option) =>
          option.targets
            .filter(
              (target) =>
                !bindings.some((binding) => binding.target_id === target.id)
            )
            .map((target) => ({
              botId: option.bot.id,
              botName: option.bot.name,
              targetId: target.id,
              targetName: target.name,
            }))
        ),
      [bindableTargets, bindings]
    );

    const suggestedTarget =
      bindings.length === 0 && availableTargets.length === 1
        ? availableTargets[0]
        : null;

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        modal.hide();
      }
    };

    const pending =
      createBinding.isPending ||
      updateBinding.isPending ||
      deleteBinding.isPending ||
      sendTestMessage.isPending;
    const mutationError =
      createBinding.error ||
      updateBinding.error ||
      deleteBinding.error ||
      sendTestMessage.error;
    const mutationErrorMessage =
      mutationError instanceof Error ? mutationError.message : null;
    const queryError = bindingsError || targetsError;
    const queryErrorMessage =
      queryError instanceof Error ? queryError.message : null;

    const buildBindingUpdate = (
      binding: WorkspaceFeishuBinding,
      overrides: Partial<UpdateWorkspaceFeishuBindingRequest>
    ): UpdateWorkspaceFeishuBindingRequest => ({
      enabled: binding.enabled,
      notify_on_status: binding.notify_on_status,
      notify_on_agent_reply: binding.notify_on_agent_reply,
      notify_on_pr: binding.notify_on_pr,
      allow_commands: binding.allow_commands,
      allow_chat_messages: binding.allow_chat_messages,
      allow_cards: binding.allow_cards,
      ack_reaction_enabled: binding.ack_reaction_enabled,
      ack_reaction_emoji_type: binding.ack_reaction_emoji_type,
      sync_group_announcement: binding.sync_group_announcement,
      ...overrides,
    });

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[720px]">
          <DialogHeader>
            <DialogTitle>{t('workspaces.feishu.dialog.title')}</DialogTitle>
            <DialogDescription>
              {t('workspaces.feishu.dialog.description')}
            </DialogDescription>
          </DialogHeader>

          {(bindingsLoading || targetsLoading) && (
            <div className="py-4 text-sm text-muted-foreground">
              {t('states.loading')}
            </div>
          )}

          {!bindingsLoading && !targetsLoading && (
            <div className="space-y-4 py-2">
              {mutationErrorMessage && (
                <Alert variant="destructive">
                  <AlertDescription>{mutationErrorMessage}</AlertDescription>
                </Alert>
              )}

              {queryErrorMessage && (
                <Alert variant="destructive">
                  <AlertDescription>{queryErrorMessage}</AlertDescription>
                </Alert>
              )}

              {suggestedTarget && (
                <Alert>
                  <AlertDescription className="flex items-center justify-between gap-4">
                    <span>
                      {t('workspaces.feishu.suggested', {
                        bot: suggestedTarget.botName,
                        target: suggestedTarget.targetName,
                      })}
                    </span>
                    <Button
                      size="sm"
                      onClick={() =>
                        createBinding.mutate({
                          bot_id: suggestedTarget.botId,
                          target_id: suggestedTarget.targetId,
                        })
                      }
                      disabled={pending}
                    >
                      {t('workspaces.feishu.actions.bind')}
                    </Button>
                  </AlertDescription>
                </Alert>
              )}

              <div className="space-y-3">
                <h3 className="text-sm font-medium text-foreground">
                  {t('workspaces.feishu.currentBindings')}
                </h3>
                {bindings.length === 0 ? (
                  <div className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
                    {t('workspaces.feishu.emptyBindings')}
                  </div>
                ) : (
                  bindings.map((binding) => {
                    const targetMeta = targetLookup.get(binding.target_id);
                    return (
                      <div
                        key={binding.id}
                        className="rounded-md border p-4 space-y-3"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div>
                            <div className="font-medium text-foreground">
                              {targetMeta?.targetName || binding.target_id}
                            </div>
                            <div className="text-sm text-muted-foreground">
                              {targetMeta?.botName || binding.bot_id}
                            </div>
                          </div>
                          <div className="flex gap-2">
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={() =>
                                sendTestMessage.mutate({
                                  bot_id: binding.bot_id,
                                  target_id: binding.target_id,
                                  message: null,
                                })
                              }
                              disabled={pending}
                            >
                              {t('workspaces.feishu.actions.test')}
                            </Button>
                            <Button
                              variant="destructive"
                              size="sm"
                              onClick={() => deleteBinding.mutate(binding.id)}
                              disabled={pending}
                            >
                              {t('workspaces.feishu.actions.unbind')}
                            </Button>
                          </div>
                        </div>

                        <div className="grid gap-3 md:grid-cols-2">
                          <BindingToggle
                            label={t('workspaces.feishu.toggles.enabled')}
                            checked={binding.enabled}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  enabled: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t(
                              'workspaces.feishu.toggles.notifyOnStatus'
                            )}
                            checked={binding.notify_on_status}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  notify_on_status: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t(
                              'workspaces.feishu.toggles.notifyOnAgentReply'
                            )}
                            checked={binding.notify_on_agent_reply}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  notify_on_agent_reply: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t('workspaces.feishu.toggles.notifyOnPr')}
                            checked={binding.notify_on_pr}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  notify_on_pr: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t('workspaces.feishu.toggles.allowCommands')}
                            checked={binding.allow_commands}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  allow_commands: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t(
                              'workspaces.feishu.toggles.allowChatMessages'
                            )}
                            checked={binding.allow_chat_messages}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  allow_chat_messages: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t('workspaces.feishu.toggles.allowCards')}
                            checked={binding.allow_cards}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  allow_cards: checked,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t(
                              'workspaces.feishu.toggles.ackReactionEnabled'
                            )}
                            checked={binding.ack_reaction_enabled}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  ack_reaction_enabled: checked,
                                }),
                              })
                            }
                          />
                          <BindingSelect
                            label={t(
                              'workspaces.feishu.toggles.ackReactionEmojiType'
                            )}
                            value={binding.ack_reaction_emoji_type || 'Typing'}
                            options={ACK_REACTION_EMOJI_OPTIONS.map(
                              (value) => ({
                                value,
                                label: t(
                                  `workspaces.feishu.reactionEmojiOptions.${value}`
                                ),
                              })
                            )}
                            onValueChange={(value) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  ack_reaction_emoji_type: value,
                                }),
                              })
                            }
                          />
                          <BindingToggle
                            label={t(
                              'workspaces.feishu.toggles.syncGroupAnnouncement'
                            )}
                            checked={binding.sync_group_announcement}
                            onCheckedChange={(checked) =>
                              updateBinding.mutate({
                                bindingId: binding.id,
                                data: buildBindingUpdate(binding, {
                                  sync_group_announcement: checked,
                                }),
                              })
                            }
                          />
                        </div>
                      </div>
                    );
                  })
                )}
              </div>

              <div className="space-y-3">
                <h3 className="text-sm font-medium text-foreground">
                  {t('workspaces.feishu.availableTargets')}
                </h3>
                {queryErrorMessage ? (
                  <div className="rounded-md border border-dashed p-4 text-sm text-destructive">
                    {queryErrorMessage}
                  </div>
                ) : availableTargets.length === 0 ? (
                  <div className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
                    {t('workspaces.feishu.emptyTargets')}
                  </div>
                ) : (
                  <div className="space-y-2">
                    {availableTargets.map((target) => (
                      <div
                        key={target.targetId}
                        className="flex items-center justify-between rounded-md border p-3"
                      >
                        <div>
                          <div className="font-medium text-foreground">
                            {target.targetName}
                          </div>
                          <div className="text-sm text-muted-foreground">
                            {target.botName}
                          </div>
                        </div>
                        <Button
                          size="sm"
                          onClick={() =>
                            createBinding.mutate({
                              bot_id: target.botId,
                              target_id: target.targetId,
                            })
                          }
                          disabled={pending}
                        >
                          {t('workspaces.feishu.actions.bind')}
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => modal.hide()}>
              {t('buttons.close')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

function BindingToggle({
  label,
  checked,
  onCheckedChange,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between rounded-md border px-3 py-2 text-sm">
      <span>{label}</span>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </label>
  );
}

function BindingSelect({
  label,
  options,
  value,
  onValueChange,
}: {
  label: string;
  options: ReadonlyArray<{ value: string; label: string }>;
  value: string;
  onValueChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-md border px-3 py-2 text-sm">
      <span>{label}</span>
      <Select value={value} onValueChange={onValueChange}>
        <SelectTrigger className="w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  );
}

export const FeishuBindingDialog = defineModal<FeishuBindingDialogProps, void>(
  FeishuBindingDialogImpl
);
