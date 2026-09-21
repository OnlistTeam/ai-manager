import { invokeNative } from "../client";
import { z } from "zod";
import {
  operationListSchema,
  operationSchema,
  type Operation,
} from "../schemas/operation";

const cancellableOperationIdSchema = z.uuid();

export const operations = {
  list(): Promise<Operation[]> {
    return invokeNative("app_operations_list", operationListSchema);
  },

  cancel(id: string): Promise<Operation> {
    const operationId = cancellableOperationIdSchema.parse(id);
    return invokeNative("app_operation_cancel", operationSchema, {
      operationId,
    });
  },
};
