import { isTerminal, type Operation } from "@/entities/operation";
import type { Tool, ToolId } from "@/entities/tool";
import {
  retryActionForOperation,
  ToolOperationProgress,
} from "@/features/tool-management";
import { ListGroup } from "@/shared/ui/ListGroup";
import { ToolCard, type ToolCardAction } from "@/shared/ui/ToolCard";

interface ToolsGridProps {
  tools: Tool[];
  cardOperations: ReadonlyMap<ToolId, Operation>;
  awaitingInventory: ReadonlySet<ToolId>;
  actionsBlocked: boolean;
  /** Background fetch filling in the latest version number; only affects a small spinner on the card, no buttons. */
  checkingVersions: boolean;
  submittingToolIds: ReadonlySet<ToolId>;
  pendingActions: ReadonlyMap<ToolId, ToolCardAction>;
  launchBusy: boolean;
  pendingToolId: ToolId | null | undefined;
  onAction: (tool: Tool, action: ToolCardAction) => void;
  onDetails?: (tool: Tool) => void;
  onDismissOperation: (operation: Operation) => void;
  onRetryOperation: (tool: Tool, operation: Operation) => void;
  onRecoverUpdate: (tool: Tool, operation: Operation, version: string) => void;
  cancellingOperationId: string | null;
  onCancelOperation: (operation: Operation) => void;
}

export function ToolsGrid({
  tools,
  cardOperations,
  awaitingInventory,
  actionsBlocked,
  checkingVersions,
  submittingToolIds,
  pendingActions,
  launchBusy,
  pendingToolId,
  onAction,
  onDetails,
  onDismissOperation,
  onRetryOperation,
  onRecoverUpdate,
  cancellingOperationId,
  onCancelOperation,
}: ToolsGridProps) {
  return (
    <ListGroup>
      {tools.map((tool) => {
        const cardOperation = cardOperations.get(tool.id);
        const operationActive = cardOperation
          ? !isTerminal(cardOperation.status)
          : false;
        const opening = launchBusy && pendingToolId === tool.id;
        return (
          <ToolCard
            key={tool.id}
            tool={tool}
            onDetails={
              tool.status !== "notInstalled" && onDetails
                ? () => onDetails(tool)
                : undefined
            }
            activeTask={
              cardOperation ? (
                <ToolOperationProgress
                  operation={cardOperation}
                  toolName={tool.name}
                  tool={tool}
                  retryDisabled={actionsBlocked}
                  onDismiss={() => onDismissOperation(cardOperation)}
                  onRetry={
                    cardOperation.updateRecovery !== null ||
                    retryActionForOperation(cardOperation) === null
                      ? undefined
                      : () => onRetryOperation(tool, cardOperation)
                  }
                  onRestoreVersion={(version) =>
                    onRecoverUpdate(tool, cardOperation, version)
                  }
                  onCancel={() => onCancelOperation(cardOperation)}
                  cancelPending={cancellingOperationId === cardOperation.id}
                />
              ) : undefined
            }
            activeTaskBusy={operationActive}
            checkingVersion={checkingVersions}
            busy={
              actionsBlocked ||
              operationActive ||
              awaitingInventory.has(tool.id) ||
              submittingToolIds.has(tool.id) ||
              opening
            }
            pendingAction={opening ? "open" : pendingActions.get(tool.id)}
            onAction={(action) => onAction(tool, action)}
          />
        );
      })}
    </ListGroup>
  );
}
