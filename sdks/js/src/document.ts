/**
 * USSL Document
 */

import type { USSLClient } from './client';
import type {
  DocumentOptions,
  Strategy,
  Value,
  Delta,
  SubscribeCallback,
  Unsubscribe,
} from './types';

/**
 * A synchronized document
 */
export class Document {
  private client: USSLClient;
  private options: DocumentOptions;
  private subscribers = new Set<SubscribeCallback>();
  private localCache: Value = null;
  private subscribed = false;

  /** Document ID */
  public readonly id: string;

  constructor(client: USSLClient, id: string, options: DocumentOptions = {}) {
    this.client = client;
    this.id = id;
    this.options = options;
  }

  /** Conflict resolution strategy */
  get strategy(): Strategy {
    return this.options.strategy || 'lww';
  }

  /**
   * Get the entire document or a specific path
   */
  async get(path?: string): Promise<Value> {
    const command = path ? `GET ${this.id} PATH ${path}` : `GET ${this.id}`;
    const response = await this.client.send(command);
    return this.parseResponse(response);
  }

  /**
   * Set a value at the specified path
   */
  async set(path: string, value: Value): Promise<void> {
    const jsonValue = JSON.stringify(value);
    await this.client.send(`SET ${this.id} ${path} ${jsonValue}`);
    this.notifySubscribers();
  }

  /**
   * Delete a path or the entire document
   */
  async delete(path?: string): Promise<void> {
    const command = path ? `DEL ${this.id} PATH ${path}` : `DEL ${this.id}`;
    await this.client.send(command);
    this.notifySubscribers();
  }

  /**
   * Push a value to an array at the specified path
   */
  async push(path: string, value: Value): Promise<void> {
    const jsonValue = JSON.stringify(value);
    await this.client.send(`PUSH ${this.id} ${path} ${jsonValue}`);
    this.notifySubscribers();
  }

  /**
   * Increment a counter at the specified path
   */
  async increment(path: string, delta: number = 1): Promise<number> {
    const response = await this.client.send(`INC ${this.id} ${path} ${delta}`);
    const match = response.match(/^:(-?\d+)$/);
    return match ? parseInt(match[1], 10) : 0;
  }

  /**
   * Subscribe to document changes
   */
  subscribe(callback: SubscribeCallback): Unsubscribe {
    this.subscribers.add(callback);

    // Subscribe to server updates if not already
    if (!this.subscribed) {
      this.subscribed = true;
      this.client.send(`SUB ${this.id}`).catch(console.error);
    }

    // Send current value immediately
    this.get().then((value) => {
      this.localCache = value;
      callback(value);
    });

    return () => {
      this.subscribers.delete(callback);
      if (this.subscribers.size === 0 && this.subscribed) {
        this.subscribed = false;
        this.client.send(`UNSUB ${this.id}`).catch(console.error);
      }
    };
  }

  /**
   * Get a Y.Text binding for collaborative text editing
   * (Placeholder - full implementation would integrate with Y.js)
   */
  text(path: string): TextBinding {
    return new TextBinding(this, path);
  }

  /**
   * Internal: Handle delta update from server
   */
  _handleDelta(delta: Delta): void {
    if (delta.documentId !== this.id) return;

    // Refresh from server and notify
    this.get().then((value) => {
      this.localCache = value;
      this.notifySubscribers(delta);
    });
  }

  /**
   * Internal: Re-subscribe after reconnection
   */
  _resubscribe(): void {
    if (this.subscribed) {
      this.client.send(`SUB ${this.id}`).catch(console.error);
    }
  }

  private notifySubscribers(delta?: Delta): void {
    this.get().then((value) => {
      this.localCache = value;
      for (const callback of this.subscribers) {
        callback(value, delta);
      }
    });
  }

  private parseResponse(response: string): Value {
    // Handle null
    if (response === '$-1') {
      return null;
    }

    // Handle bulk string
    if (response.startsWith('$')) {
      // Format: $<length>\r\n<data>
      // But we receive it as a single line after parsing
      const match = response.match(/^\$\d+$/);
      if (match) {
        // Need to wait for next line with data
        return null;
      }
    }

    // Handle JSON data (most common)
    try {
      // Response might be: $<len>\r\n<json>
      const jsonMatch = response.match(/^\$\d+\r?\n?(.*)$/s);
      if (jsonMatch) {
        return JSON.parse(jsonMatch[1]);
      }
      return JSON.parse(response);
    } catch {
      // Return as string if not JSON
      return response;
    }
  }
}

/**
 * Text binding for collaborative text editing
 */
class TextBinding {
  private doc: Document;
  private path: string;

  constructor(doc: Document, path: string) {
    this.doc = doc;
    this.path = path;
  }

  /**
   * Insert text at position
   */
  async insert(index: number, text: string): Promise<void> {
    const current = (await this.doc.get(this.path)) as string || '';
    const newText = current.slice(0, index) + text + current.slice(index);
    await this.doc.set(this.path, newText);
  }

  /**
   * Delete text range
   */
  async delete(index: number, length: number): Promise<void> {
    const current = (await this.doc.get(this.path)) as string || '';
    const newText = current.slice(0, index) + current.slice(index + length);
    await this.doc.set(this.path, newText);
  }

  /**
   * Get current text
   */
  async toString(): Promise<string> {
    return (await this.doc.get(this.path)) as string || '';
  }
}
