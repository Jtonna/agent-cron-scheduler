import { SavedConnection } from "./types";

const CONNECTIONS_KEY = "acs-connections";
const ACTIVE_CONNECTION_KEY = "acs-active-connection";

/**
 * Get all saved connections from localStorage.
 * Returns an empty array if no connections are stored.
 */
export function getConnections(): SavedConnection[] {
  try {
    const stored = localStorage.getItem(CONNECTIONS_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch {
    return [];
  }
}

/**
 * Save a new connection to localStorage.
 */
export function saveConnection(conn: SavedConnection): void {
  const connections = getConnections();
  connections.push(conn);
  localStorage.setItem(CONNECTIONS_KEY, JSON.stringify(connections));
}

/**
 * Update an existing connection by id.
 * Only updates the label and/or url fields.
 */
export function updateConnection(
  id: string,
  patch: Partial<Pick<SavedConnection, "label" | "url">>
): void {
  const connections = getConnections();
  const index = connections.findIndex((conn) => conn.id === id);
  if (index === -1) {
    return;
  }
  connections[index] = { ...connections[index], ...patch };
  localStorage.setItem(CONNECTIONS_KEY, JSON.stringify(connections));
}

/**
 * Delete a connection by id.
 * If the deleted connection is the active one, clears the active connection.
 */
export function deleteConnection(id: string): void {
  const connections = getConnections();
  const filtered = connections.filter((conn) => conn.id !== id);
  localStorage.setItem(CONNECTIONS_KEY, JSON.stringify(filtered));

  // If the deleted connection was active, clear the active connection
  if (getActiveConnectionId() === id) {
    setActiveConnectionId(null);
  }
}

/**
 * Get the id of the currently active connection.
 * Returns null if no connection is active (implying the "local" connection).
 */
export function getActiveConnectionId(): string | null {
  return localStorage.getItem(ACTIVE_CONNECTION_KEY);
}

/**
 * Set the active connection id.
 * Pass null to clear the active connection (reverting to "local").
 */
export function setActiveConnectionId(id: string | null): void {
  if (id === null) {
    localStorage.removeItem(ACTIVE_CONNECTION_KEY);
  } else {
    localStorage.setItem(ACTIVE_CONNECTION_KEY, id);
  }
}

/**
 * Get the full active connection object.
 * Returns null if no connection is active or if the active id doesn't match any saved connection.
 */
export function getActiveConnection(): SavedConnection | null {
  const activeId = getActiveConnectionId();
  if (!activeId) {
    return null;
  }
  const connections = getConnections();
  return connections.find((conn) => conn.id === activeId) ?? null;
}
