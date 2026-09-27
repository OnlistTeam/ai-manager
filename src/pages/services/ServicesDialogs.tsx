import type {
  Provider,
  ProviderDraft,
  ProviderEditProfile,
} from "@/entities/provider";
import {
  ProviderFormModal,
  ProviderRemovalModal,
} from "@/features/provider-management";

interface ServicesDialogsProps {
  editing: Provider | null;
  editingProfile: ProviderEditProfile | null;
  editingProfileLoading: boolean;
  editingProfileError: Error | null;
  removing: Provider | null;
  toolName: string;
  saveBusy: boolean;
  saveError: Error | null;
  removeBusy: boolean;
  removeError: Error | null;
  mutationsBlocked: boolean;
  onEditingOpenChange: (open: boolean) => void;
  onEditingErrorReset: () => void;
  onRemovalOpenChange: (open: boolean) => void;
  onSave: (draft: ProviderDraft) => void;
  onRemove: () => void;
}

export function ServicesDialogs({
  editing,
  editingProfile,
  editingProfileLoading,
  editingProfileError,
  removing,
  toolName,
  saveBusy,
  saveError,
  removeBusy,
  removeError,
  mutationsBlocked,
  onEditingOpenChange,
  onEditingErrorReset,
  onRemovalOpenChange,
  onSave,
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
