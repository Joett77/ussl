/**
 * USSL Presence
 */

import type { USSLClient } from './client';
import type { PresenceData, Unsubscribe } from './types';

/**
 * Presence manager for tracking connected clients
 */
export class Presence {
  private client: USSLClient;
  private subscribers = new Map<string, Set<(presence: PresenceData[]) => void>>();
  private localData: Record<string, unknown> = {};

  constructor(client: USSLClient) {
    this.client = client;
  }

  /**
   * Set local presence data for a document
   */
  async set(documentId: string, data: Record<string, unknown>): Promise<void> {
    this.localData = data;
    const jsonData = JSON.stringify(data);
    await this.client.send(`PRESENCE ${documentId} DATA ${jsonData}`);
  }

  /**
   * Get presence data for a document
   */
  async get(documentId: string): Promise<PresenceData[]> {
    const response = await this.client.send(`PRESENCE ${documentId}`);
    return this.parsePresence(response);
  }

  /**
   * Subscribe to presence changes for a document
   */
  subscribe(documentId: string, callback: (presence: PresenceData[]) => void): Unsubscribe {
    if (!this.subscribers.has(documentId)) {
      this.subscribers.set(documentId, new Set());
    }
    this.subscribers.get(documentId)!.add(callback);

    // Get initial presence
    this.get(documentId).then(callback);

    return () => {
      this.subscribers.get(documentId)?.delete(callback);
    };
  }

  private parsePresence(response: string): PresenceData[] {
    try {
      // Response is bulk data containing JSON array
      const match = response.match(/^\$\d+\r?\n?(.*)$/s);
      if (match) {
        return JSON.parse(match[1]);
      }
      return JSON.parse(response);
    } catch {
      return [];
    }
  }
}
