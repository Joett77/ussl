/**
 * USSL JavaScript/TypeScript Client SDK
 *
 * @example
 * ```typescript
 * import { USSL } from '@ussl/client';
 *
 * const client = USSL.connect('ws://localhost:6381');
 *
 * const doc = client.doc('user:123', { strategy: 'lww' });
 * doc.set('name', 'Alice');
 *
 * doc.subscribe((value) => {
 *   console.log('Updated:', value);
 * });
 * ```
 */

export { USSLClient, USSL } from './client';
export { Document } from './document';
export { Presence } from './presence';
export type {
  USSLOptions,
  Strategy,
  DocumentOptions,
  Value,
  Delta,
  PresenceData,
} from './types';
