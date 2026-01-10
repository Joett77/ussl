/**
 * USSL Client
 */

import { Document } from './document';
import { Presence } from './presence';
import type {
  USSLOptions,
  ConnectionState,
  DocumentOptions,
  Strategy,
  Delta,
} from './types';

const DEFAULT_OPTIONS: Required<USSLOptions> = {
  reconnect: true,
  maxReconnectAttempts: 10,
  reconnectDelay: 1000,
  maxReconnectDelay: 30000,
  timeout: 5000,
  WebSocket: typeof WebSocket !== 'undefined' ? WebSocket : (undefined as any),
};

/**
 * USSL Client for connecting to a USSL server
 */
export class USSLClient {
  private url: string;
  private options: Required<USSLOptions>;
  private ws: WebSocket | null = null;
  private state: ConnectionState = 'disconnected';
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private documents = new Map<string, Document>();
  private pendingCommands: Array<{
    command: string;
    resolve: (response: string) => void;
    reject: (error: Error) => void;
  }> = [];
  private responseBuffer = '';
  private eventListeners = new Map<string, Set<Function>>();

  /** Presence manager */
  public readonly presence: Presence;

  constructor(url: string, options: USSLOptions = {}) {
    this.url = url;
    this.options = { ...DEFAULT_OPTIONS, ...options };
    this.presence = new Presence(this);
  }

  /** Current connection state */
  get connectionState(): ConnectionState {
    return this.state;
  }

  /** Whether the client is connected */
  get isConnected(): boolean {
    return this.state === 'connected';
  }

  /**
   * Connect to the USSL server
   */
  async connect(): Promise<void> {
    if (this.state === 'connected' || this.state === 'connecting') {
      return;
    }

    this.state = 'connecting';

    return new Promise((resolve, reject) => {
      const WS = this.options.WebSocket;
      if (!WS) {
        reject(new Error('WebSocket not available. Provide a WebSocket implementation in options.'));
        return;
      }

      try {
        this.ws = new WS(this.url);
      } catch (error) {
        this.state = 'disconnected';
        reject(error);
        return;
      }

      const timeout = setTimeout(() => {
        if (this.state === 'connecting') {
          this.ws?.close();
          reject(new Error('Connection timeout'));
        }
      }, this.options.timeout);

      this.ws.onopen = () => {
        clearTimeout(timeout);
        this.state = 'connected';
        this.reconnectAttempt = 0;
        this.emit('connect');
        resolve();
      };

      this.ws.onclose = () => {
        clearTimeout(timeout);
        this.handleDisconnect();
      };

      this.ws.onerror = (event) => {
        clearTimeout(timeout);
        const error = new Error('WebSocket error');
        this.emit('error', error);
        if (this.state === 'connecting') {
          reject(error);
        }
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event.data as string);
      };
    });
  }

  /**
   * Disconnect from the server
   */
  disconnect(): void {
    this.options.reconnect = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.state = 'disconnected';
    this.emit('disconnect');
  }

  /**
   * Get or create a document
   */
  doc(id: string, options: DocumentOptions = {}): Document {
    let doc = this.documents.get(id);
    if (!doc) {
      doc = new Document(this, id, options);
      this.documents.set(id, doc);
    }
    return doc;
  }

  /**
   * Send a raw command to the server
   */
  async send(command: string): Promise<string> {
    if (!this.isConnected || !this.ws) {
      throw new Error('Not connected');
    }

    return new Promise((resolve, reject) => {
      this.pendingCommands.push({ command, resolve, reject });
      this.ws!.send(command + '\r\n');
    });
  }

  /**
   * Add event listener
   */
  on(event: string, callback: Function): () => void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, new Set());
    }
    this.eventListeners.get(event)!.add(callback);

    return () => {
      this.eventListeners.get(event)?.delete(callback);
    };
  }

  private emit(event: string, ...args: any[]): void {
    this.eventListeners.get(event)?.forEach((cb) => cb(...args));
  }

  private handleMessage(data: string): void {
    this.responseBuffer += data;

    // Process complete lines
    const lines = this.responseBuffer.split('\r\n');
    this.responseBuffer = lines.pop() || '';

    for (const line of lines) {
      if (!line) continue;

      // Check for delta updates (push from server)
      if (line.startsWith('#')) {
        this.handleDelta(line);
        continue;
      }

      // Handle response to pending command
      const pending = this.pendingCommands.shift();
      if (pending) {
        if (line.startsWith('-ERR')) {
          pending.reject(new Error(line.slice(5)));
        } else {
          pending.resolve(line);
        }
      }
    }
  }

  private handleDelta(line: string): void {
    // Format: #<version> <base64-data>
    const match = line.match(/^#(\d+)\s+(.+)$/);
    if (!match) return;

    const version = parseInt(match[1], 10);
    const data = this.base64ToBytes(match[2]);

    // Notify all subscribed documents
    for (const doc of this.documents.values()) {
      doc._handleDelta({ documentId: doc.id, version, data });
    }
  }

  private handleDisconnect(): void {
    this.state = 'disconnected';
    this.ws = null;

    // Reject pending commands
    for (const pending of this.pendingCommands) {
      pending.reject(new Error('Disconnected'));
    }
    this.pendingCommands = [];

    this.emit('disconnect');

    // Attempt reconnection
    if (this.options.reconnect && this.reconnectAttempt < this.options.maxReconnectAttempts) {
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    this.state = 'reconnecting';
    this.reconnectAttempt++;

    const delay = Math.min(
      this.options.reconnectDelay * Math.pow(2, this.reconnectAttempt - 1),
      this.options.maxReconnectDelay
    );

    this.emit('reconnecting', this.reconnectAttempt);

    this.reconnectTimer = setTimeout(async () => {
      try {
        await this.connect();
        // Re-subscribe documents
        for (const doc of this.documents.values()) {
          doc._resubscribe();
        }
      } catch {
        // Will trigger another reconnect via handleDisconnect
      }
    }, delay);
  }

  private base64ToBytes(base64: string): Uint8Array {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
}

/**
 * Static factory for creating USSL clients
 */
export const USSL = {
  /**
   * Connect to a USSL server
   */
  async connect(url: string, options?: USSLOptions): Promise<USSLClient> {
    const client = new USSLClient(url, options);
    await client.connect();
    return client;
  },
};
