import type {
  Provider,
  ProviderConnectionProfile,
  ProviderCreateDraft,
  ProviderCustomCreateDraft,
  ProviderDraft,
  ProviderEditProfile,
  ToolId,
} from "@/entities/provider";
import {
  ProviderConnectModal,
  ProviderFormModal,
  ProviderRemovalModal,
} from "@/features/provider-management";

interface ServicesDialogsProps {
  editing: Provider | null;
  editingProfile: ProviderEditProfile | null;
  editingProfileLoading: boolean;
  editingProfileError: Error | null;
  connectingProfile: ProviderConnectionProfile | null;
  tool: ToolId | null;
  removing: Provider | null;
  toolName: string;
  saveBusy: boolean;
  saveError: Error | null;
  createBusy: boolean;
  createError: Error | null;
  removeBusy: boolean;
  removeError: Error | null;
  mutationsBlocked: boolean;
  preferCompatibleConnection: boolean;
  onEditingOpenChange: (open: boolean) => void;
  onEditingErrorReset: () => void;
  onConnectingOpenChange: (open: boolean) => void;
  onConnectingErrorReset: () => void;
  onRemovalOpenChange: (open: boolean) => void;
  onSave: (draft: ProviderDraft) => void;
  onCreate: (draft: ProviderCreateDraft) => void;
  onCreateCustom: (draft: ProviderCustomCreateDraft) => void;
  onRemove: () => void;
}

export function ServicesDialogs({
  editing,
  editingProfile,
  editingProfileLoading,
  editingProfileError,
  connectingProfile,
  tool,
  removing,
  toolName,
  saveBusy,
  saveError,
  createBusy,
  createError,
  removeBusy,
  removeError,
  mutationsBlocked,
  preferCompatibleConnection,
  onEditingOpenChange,
  onEditingErrorReset,
  onConnectingOpenChange,
  onConnectingErrorReset,
  onRemovalOpenChange,
  onSave,
  onCreate,
  onCreateCustom,
  onRemove,
}: ServicesDialogsProps) {
  return (
    <>
      <ProviderFormModal
        provider={editing}
        profile={editingProfile}
        profileLoading={editingProfileLoading}
        profileError={editingProfileError}
        busy={saveBusy}
        error={saveError}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onEditingOpenChange}
        onErrorReset={onEditingErrorReset}
        onSubmit={onSave}
      />
      <ProviderConnectModal
        profile={connectingProfile}
        tool={tool}
        toolName={toolName}
        busy={createBusy}
        error={createError}
        mutationsBlocked={mutationsBlocked}
        preferCompatible={preferCompatibleConnection}
        onOpenChange={onConnectingOpenChange}
        onErrorReset={onConnectingErrorReset}
        onSubmit={onCreate}
        onCustomSubmit={onCreateCustom}
      />
      <ProviderRemovalModal
        provider={removing}
        toolName={toolName}
        busy={removeBusy}
        error={removeError}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onRemovalOpenChange}
        onConfirm={onRemove}
      />
    </>
  );
}
