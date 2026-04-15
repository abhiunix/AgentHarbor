/**
 * Generates a stable opaque composite ID for new custom capabilities/agents.
 * Format: authorId/suffix (suffix is lowercase hex, kebab-safe for CompositeId).
 */
export function makeStableCompositeId(authorId: string): string {
  const suffix = randomHex(8);
  return `${authorId}/${suffix}`;
}

function randomHex(length: number): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

const MAX_RETRIES = 3;

/**
 * Generate a stable ID and optionally retry if the given ID is already present.
 * Use when creating a new capability/agent to avoid overwriting existing items.
 */
export async function makeStableCompositeIdWithRetry(
  authorId: string,
  existingIds: Set<string>
): Promise<string> {
  for (let i = 0; i < MAX_RETRIES; i++) {
    const id = makeStableCompositeId(authorId);
    if (!existingIds.has(id)) return id;
  }
  return makeStableCompositeId(authorId);
}
