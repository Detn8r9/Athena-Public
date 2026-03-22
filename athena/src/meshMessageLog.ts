import type { MeshRadioMessage } from "./MeshRadioModal";

/** Public mesh log in the UI — matches backend/channel expectations for bounded memory. */
export const MESH_LOG_MAX_LINES = 500;

export function appendMeshMessageCapped(
  prev: MeshRadioMessage[],
  incoming: MeshRadioMessage,
): MeshRadioMessage[] {
  const next = [...prev, incoming];
  return next.length > MESH_LOG_MAX_LINES ? next.slice(-MESH_LOG_MAX_LINES) : next;
}
