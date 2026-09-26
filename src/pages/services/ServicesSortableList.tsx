import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type Announcements,
  type DragEndEvent,
  type UniqueIdentifier,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical } from "lucide-react";
import { Fragment, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Provider } from "@/entities/provider";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

interface ServicesSortableListProps {
  providers: readonly Provider[];
  /** Blocks picking anything up, e.g. while another service action runs. */
  disabled: boolean;
  onReorder: (providerIds: string[]) => void;
  /** Renders one card; `handle` is `null` when there is nothing to reorder. */
  children: (provider: Provider, handle: ReactNode) => ReactNode;
}

interface SortableRowProps {
  provider: Provider;
  disabled: boolean;
  children: (handle: ReactNode) => ReactNode;
}

function SortableRow({ provider, disabled, children }: SortableRowProps) {
  const { t } = useTranslation();
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({
    id: provider.id,
    disabled,
    attributes: { roleDescription: t("services.reorder.roleDescription") },
  });
  const label = t("services.reorder.handle", { name: provider.name });
  const handle = (
    <button
      ref={setActivatorNodeRef}
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      className={cn(
        "-mr-2 flex h-8 w-6 shrink-0 cursor-grab touch-none items-center justify-center rounded-md",
        "text-content-muted/60 transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
        "disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent",
        isDragging && "cursor-grabbing text-content",
        FOCUS_RING,
      )}
      {...attributes}
      {...listeners}
    >
      <GripVertical className="h-4 w-4" aria-hidden="true" />
    </button>
  );

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      // A row lifted out of the list needs its own opaque surface, or the
      // rows it passes over would show through it.
      className={cn(
        "min-w-0",
        isDragging && "app-floating-surface relative z-10 rounded-lg",
      )}
    >
      {children(handle)}
    </div>
  );
}

/**
 * The saved services in the user's own order. A card only moves when the user
 * moves it: by pointer on the handle, or from the keyboard (Space/Enter to pick
 * up, arrows to move, Space/Enter to drop, Escape to cancel).
 */
export function ServicesSortableList({
  providers,
  disabled,
  onReorder,
  children,
}: ServicesSortableListProps) {
  const { t } = useTranslation();
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );
  const ids = providers.map((provider) => provider.id);
  const describe = (
    key: string,
    activeId: UniqueIdentifier,
    positionId: UniqueIdentifier = activeId,
  ) =>
    t(key, {
      name: providers[ids.indexOf(String(activeId))]?.name ?? "",
      position: ids.indexOf(String(positionId)) + 1,
      count: ids.length,
    });
  const announcements: Announcements = {
    onDragStart: ({ active }) => describe("services.reorder.picked", active.id),
    onDragOver: ({ active, over }) =>
      over ? describe("services.reorder.moved", active.id, over.id) : undefined,
    onDragEnd: ({ active, over }) =>
      over
        ? describe("services.reorder.dropped", active.id, over.id)
        : describe("services.reorder.cancelled", active.id),
    onDragCancel: ({ active }) =>
      describe("services.reorder.cancelled", active.id),
  };

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const from = ids.indexOf(String(active.id));
    const to = ids.indexOf(String(over.id));
    if (from < 0 || to < 0) return;
    onReorder(arrayMove(ids, from, to));
  };

  // Nothing to reorder: no handle, and no drag instructions or live region.
  if (providers.length < 2) {
    return providers.map((provider) => (
      <Fragment key={provider.id}>{children(provider, null)}</Fragment>
    ));
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      accessibility={{
        announcements,
        screenReaderInstructions: {
          draggable: t("services.reorder.instructions"),
        },
      }}
      onDragEnd={handleDragEnd}
    >
      <SortableContext items={ids} strategy={verticalListSortingStrategy}>
        {providers.map((provider) => (
          <SortableRow
            key={provider.id}
            provider={provider}
            disabled={disabled}
          >
            {(handle) => children(provider, handle)}
          </SortableRow>
        ))}
      </SortableContext>
    </DndContext>
  );
}
