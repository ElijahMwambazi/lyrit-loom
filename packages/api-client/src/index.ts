import createClient from "openapi-fetch";

import type { paths } from "./schema";

export type { components, operations, paths } from "./schema";

export function createApiClient(baseUrl = "/api/v1") {
  return createClient<paths>({ baseUrl });
}
