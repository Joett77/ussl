/**
 * USSL Type Definitions
 */

/** Conflict resolution strategy */
export type Strategy =
  | 'lww'
  | 'crdt-counter'
  | 'crdt-set'
  | 'crdt-map'
  | 'crdt-text';

/** JSON-compatible value types */
export type Value =
  | null
  | boolean
  | number
  | string
  | Value[]
  | { [key: string]: Value };

/** Delta update from server */
export interface Delta {
  documentId: string;
  version: number;
  path?: string;
  data: Uint8Array;
}

/** Presence data for a client */
export interface PresenceData {
  clientId: string;
  data: Record<string, unknown>;
}

/** Connection options */
export interface USSLOptions {
  /** Reconnection settings */
  reconnect?: boolean;
  /** Maximum reconnection attempts */
  maxReconnectAttempts?: number;
  /** Initial reconnection delay in ms */
  reconnectDelay?: number;
  /** Maximum reconnection delay in ms */
  maxReconnectDelay?: number;
  /** Connection timeout in ms */
  timeout?: number;
  /** Custom WebSocket implementation (for Node.js) */
  WebSocket?: typeof WebSocket;
}

/** Document options */
export interface DocumentOptions {
  /** Conflict resolution strategy */
  strategy?: Strategy;
  /** Time-to-live in milliseconds */
  ttl?: number;
}

/** Connection state */
export type ConnectionState =
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'reconnecting';

/** Event types */
export interface USSLEvents {
  connect: () => void;
  disconnect: () => void;
  error: (error: Error) => void;
  reconnecting: (attempt: number) => void;
}

/** Subscription callback */
export type SubscribeCallback<T = Value> = (value: T, delta?: Delta) => void;

/** Unsubscribe function */
export type Unsubscribe = () => void;
