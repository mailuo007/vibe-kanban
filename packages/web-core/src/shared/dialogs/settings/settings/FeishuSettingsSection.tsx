import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Alert, AlertDescription } from '@vibe/ui/components/Alert';
import { Button } from '@vibe/ui/components/Button';
import { cn } from '@/shared/lib/utils';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsSelect,
} from './SettingsComponents';
import {
  useCreateFeishuBot,
  useDeleteFeishuBot,
  useDiscoverFeishuTargets,
  useFeishuBots,
  useFeishuTargets,
  useUpdateFeishuBot,
  useValidateFeishuBot,
} from '@/shared/hooks/useFeishuBots';

type TenantMode = 'self_built';

type FormState = {
  name: string;
  appId: string;
  appSecret: string;
  encryptKey: string;
  verificationToken: string;
  tenantMode: TenantMode;
  enabled: boolean;
};

const EMPTY_FORM: FormState = {
  name: '',
  appId: '',
  appSecret: '',
  encryptKey: '',
  verificationToken: '',
  tenantMode: 'self_built',
  enabled: true,
};

export function FeishuSettingsSection() {
  const { t } = useTranslation('settings');
  const { data: bots = [], isLoading } = useFeishuBots();
  const createBot = useCreateFeishuBot();
  const updateBot = useUpdateFeishuBot();
  const deleteBot = useDeleteFeishuBot();
  const validateBot = useValidateFeishuBot();
  const discoverTargets = useDiscoverFeishuTargets();

  const [selectedBotId, setSelectedBotId] = useState<string | null>(null);
  const [isCreatingBot, setIsCreatingBot] = useState(false);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedBot = useMemo(
    () => bots.find((bot) => bot.id === selectedBotId) ?? null,
    [bots, selectedBotId]
  );

  const { data: targets = [] } = useFeishuTargets(selectedBot?.id);
  const isEditingBot = !!selectedBot && !isCreatingBot;

  useEffect(() => {
    if (!isCreatingBot && !selectedBotId && bots.length > 0) {
      setSelectedBotId(bots[0].id);
    }
  }, [bots, isCreatingBot, selectedBotId]);

  useEffect(() => {
    if (isCreatingBot) {
      setForm(EMPTY_FORM);
      return;
    }

    if (!selectedBot) {
      setForm(EMPTY_FORM);
      return;
    }

    setForm({
      name: selectedBot.name,
      appId: selectedBot.app_id,
      appSecret: '',
      encryptKey: '',
      verificationToken: '',
      tenantMode: 'self_built',
      enabled: selectedBot.enabled,
    });
  }, [isCreatingBot, selectedBot]);

  const isSubmitting =
    createBot.isPending ||
    updateBot.isPending ||
    deleteBot.isPending ||
    validateBot.isPending ||
    discoverTargets.isPending;

  const handleSave = async () => {
    setError(null);
    setFeedback(null);

    try {
      if (isEditingBot) {
        await updateBot.mutateAsync({
          botId: selectedBot.id,
          data: {
            name: form.name,
            app_id: form.appId,
            app_secret: form.appSecret || null,
            encrypt_key: form.encryptKey || null,
            verification_token: form.verificationToken || null,
            tenant_mode: form.tenantMode,
            enabled: form.enabled,
          },
        });
        setFeedback(t('settings.feishu.messages.updated'));
      } else {
        const createdBot = await createBot.mutateAsync({
          name: form.name,
          app_id: form.appId,
          app_secret: form.appSecret,
          encrypt_key: form.encryptKey || null,
          verification_token: form.verificationToken || null,
          tenant_mode: form.tenantMode,
        });
        setIsCreatingBot(false);
        setSelectedBotId(createdBot.id);
        setFeedback(t('settings.feishu.messages.created'));
      }
    } catch (mutationError) {
      setError(
        mutationError instanceof Error
          ? mutationError.message
          : t('settings.feishu.messages.saveError')
      );
    }
  };

  const handleValidate = async () => {
    if (!selectedBot) return;
    setError(null);
    setFeedback(null);
    try {
      const result = await validateBot.mutateAsync(selectedBot.id);
      setFeedback(result.validation.message);
    } catch (mutationError) {
      setError(
        mutationError instanceof Error
          ? mutationError.message
          : t('settings.feishu.messages.validateError')
      );
    }
  };

  const handleRefreshTargets = async () => {
    if (!selectedBot) return;
    setError(null);
    setFeedback(null);
    try {
      const refreshedTargets = await discoverTargets.mutateAsync(
        selectedBot.id
      );
      setFeedback(
        t('settings.feishu.messages.targetsRefreshed', {
          count: refreshedTargets.length,
        })
      );
    } catch (mutationError) {
      setError(
        mutationError instanceof Error
          ? mutationError.message
          : t('settings.feishu.messages.refreshError')
      );
    }
  };

  const handleDelete = async () => {
    if (!selectedBot) return;
    setError(null);
    setFeedback(null);
    try {
      await deleteBot.mutateAsync(selectedBot.id);
      setIsCreatingBot(false);
      setSelectedBotId(null);
      setForm(EMPTY_FORM);
      setFeedback(t('settings.feishu.messages.deleted'));
    } catch (mutationError) {
      setError(
        mutationError instanceof Error
          ? mutationError.message
          : t('settings.feishu.messages.deleteError')
      );
    }
  };

  return (
    <div className="space-y-6">
      <SettingsCard
        title={t('settings.feishu.title')}
        description={t('settings.feishu.description')}
        headerAction={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setIsCreatingBot(true);
              setSelectedBotId(null);
              setForm(EMPTY_FORM);
              setError(null);
              setFeedback(null);
            }}
          >
            {t('settings.feishu.actions.newBot')}
          </Button>
        }
      >
        {(error || feedback) && (
          <Alert variant={error ? 'destructive' : 'default'}>
            <AlertDescription>{error || feedback}</AlertDescription>
          </Alert>
        )}

        <div className="grid gap-4 lg:grid-cols-[240px_minmax(0,1fr)]">
          <div className="rounded-sm border border-border bg-secondary/30">
            <div className="border-b border-border px-3 py-2 text-xs font-medium uppercase tracking-wide text-low">
              {t('settings.feishu.botList.title')}
            </div>
            <div className="max-h-[520px] overflow-y-auto">
              {isLoading ? (
                <div className="px-3 py-4 text-sm text-low">
                  {t('settings.feishu.botList.loading')}
                </div>
              ) : bots.length === 0 ? (
                <div className="px-3 py-4 text-sm text-low">
                  {t('settings.feishu.botList.empty')}
                </div>
              ) : (
                bots.map((bot) => (
                  <button
                    key={bot.id}
                    className={cn(
                      'flex w-full flex-col items-start gap-1 border-b border-border px-3 py-3 text-left last:border-b-0 hover:bg-primary/5',
                      selectedBotId === bot.id && 'bg-brand/10'
                    )}
                    onClick={() => {
                      setIsCreatingBot(false);
                      setSelectedBotId(bot.id);
                      setError(null);
                      setFeedback(null);
                    }}
                  >
                    <div className="flex w-full items-center justify-between gap-2">
                      <span className="truncate text-sm font-medium text-high">
                        {bot.name}
                      </span>
                      <span
                        className={cn(
                          'rounded-full px-2 py-0.5 text-[11px] font-medium',
                          bot.enabled
                            ? 'bg-brand/15 text-brand'
                            : 'bg-muted text-low'
                        )}
                      >
                        {bot.last_health_status}
                      </span>
                    </div>
                    <span className="truncate text-xs text-low">
                      {bot.app_id}
                    </span>
                    {bot.last_error && (
                      <span className="line-clamp-2 text-xs text-error">
                        {bot.last_error}
                      </span>
                    )}
                  </button>
                ))
              )}
            </div>
          </div>

          <div className="space-y-4">
            <SettingsField label={t('settings.feishu.form.name.label')}>
              <SettingsInput
                value={form.name}
                onChange={(value) =>
                  setForm((current) => ({ ...current, name: value }))
                }
                placeholder={t('settings.feishu.form.name.placeholder')}
                disabled={isSubmitting}
              />
            </SettingsField>

            <SettingsField label={t('settings.feishu.form.appId.label')}>
              <SettingsInput
                value={form.appId}
                onChange={(value) =>
                  setForm((current) => ({ ...current, appId: value }))
                }
                placeholder={t('settings.feishu.form.appId.placeholder')}
                disabled={isSubmitting}
              />
            </SettingsField>

            <div className="grid gap-4 md:grid-cols-2">
              <SettingsField label={t('settings.feishu.form.appSecret.label')}>
                <SettingsInput
                  value={form.appSecret}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, appSecret: value }))
                  }
                  placeholder={t('settings.feishu.form.appSecret.placeholder')}
                  disabled={isSubmitting}
                />
              </SettingsField>

              <SettingsField label={t('settings.feishu.form.tenantMode.label')}>
                <SettingsSelect<TenantMode>
                  value={form.tenantMode}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, tenantMode: value }))
                  }
                  options={[
                    {
                      value: 'self_built',
                      label: t('settings.feishu.form.tenantMode.selfBuilt'),
                    },
                  ]}
                  disabled={isSubmitting}
                />
              </SettingsField>
            </div>

            <div className="grid gap-4 md:grid-cols-2">
              <SettingsField label={t('settings.feishu.form.encryptKey.label')}>
                <SettingsInput
                  value={form.encryptKey}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, encryptKey: value }))
                  }
                  placeholder={t('settings.feishu.form.encryptKey.placeholder')}
                  disabled={isSubmitting}
                />
              </SettingsField>

              <SettingsField
                label={t('settings.feishu.form.verificationToken.label')}
              >
                <SettingsInput
                  value={form.verificationToken}
                  onChange={(value) =>
                    setForm((current) => ({
                      ...current,
                      verificationToken: value,
                    }))
                  }
                  placeholder={t(
                    'settings.feishu.form.verificationToken.placeholder'
                  )}
                  disabled={isSubmitting}
                />
              </SettingsField>
            </div>

            <SettingsCheckbox
              id="feishu-enabled"
              label={t('settings.feishu.form.enabled.label')}
              description={t('settings.feishu.form.enabled.description')}
              checked={form.enabled}
              onChange={(checked) =>
                setForm((current) => ({ ...current, enabled: checked }))
              }
              disabled={isSubmitting}
            />

            {isEditingBot && (
              <div className="rounded-sm border border-border bg-secondary/30 p-3">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <div>
                    <h4 className="text-sm font-medium text-high">
                      {t('settings.feishu.targets.title')}
                    </h4>
                    <p className="text-xs text-low">
                      {t('settings.feishu.targets.description', {
                        count: targets.length,
                      })}
                    </p>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleRefreshTargets}
                    disabled={isSubmitting}
                  >
                    {t('settings.feishu.actions.refreshTargets')}
                  </Button>
                </div>

                <div className="max-h-40 space-y-2 overflow-y-auto">
                  {targets.length === 0 ? (
                    <p className="text-sm text-low">
                      {t('settings.feishu.targets.empty')}
                    </p>
                  ) : (
                    targets.map((target) => (
                      <div
                        key={target.id}
                        className="rounded-sm border border-border bg-panel px-3 py-2"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="truncate text-sm text-high">
                            {target.name}
                          </span>
                          <span className="text-xs text-low">
                            {target.is_active
                              ? t('settings.feishu.targets.active')
                              : t('settings.feishu.targets.inactive')}
                          </span>
                        </div>
                        <div className="text-xs text-low">
                          {target.open_chat_id ||
                            target.chat_id ||
                            target.target_type}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              <Button onClick={handleSave} disabled={isSubmitting}>
                {isEditingBot
                  ? t('settings.feishu.actions.save')
                  : t('settings.feishu.actions.create')}
              </Button>
              {isEditingBot && (
                <>
                  <Button
                    variant="outline"
                    onClick={handleValidate}
                    disabled={isSubmitting}
                  >
                    {t('settings.feishu.actions.validate')}
                  </Button>
                  <Button
                    variant="destructive"
                    onClick={handleDelete}
                    disabled={isSubmitting}
                  >
                    {t('settings.feishu.actions.delete')}
                  </Button>
                </>
              )}
            </div>
          </div>
        </div>
      </SettingsCard>
    </div>
  );
}

export { FeishuSettingsSection as FeishuSettingsSectionContent };
