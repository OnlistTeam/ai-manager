import { QueryClient } from "@tanstack/react-query";
import { sessionCacheOptions } from "./sessionCache";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      ...sessionCacheOptions,
    },
    mutations: {
      retry: false,
    },
  },
});
