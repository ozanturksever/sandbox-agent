/**
 * Convex Event Streamer for sandbox-agent
 *
 * Streams UniversalEvents from sandbox-agent to Convex mutations in real-time,
 * providing persistence and enabling UI updates.
 */

import type { UniversalEvent } from '../../types.ts';
import type {
  ConvexEventStreamConfig,
  ConvexClientLike,
  OOSSContext,
  OOSSEnrichedEvent,
} from './types.ts';

/**
 * Convex Event Streamer
 *
 * Batches and streams UniversalEvents to Convex mutations for persistence and UI.
 */
export class ConvexEventStreamer {
  private config: Required<Omit<ConvexEventStreamConfig, 'convexClient'>> & {
    convexClient: ConvexClientLike;
  };
  private buffer: OOSSEnrichedEvent[] = [];
  private flushTimeout?: ReturnType<typeof setTimeout>;
  private isFlushing = false;
  private isConnected = true;
  private oossContext?: OOSSContext;

  constructor(config: ConvexEventStreamConfig) {
    this.config = {
      convexClient: config.convexClient,
      eventMutation: config.eventMutation,
      batchSize: config.batchSize ?? 5,
      flushIntervalMs: config.flushIntervalMs ?? 500,
      resilient: config.resilient ?? true,
      maxBufferSize: config.maxBufferSize ?? 1000,
    };
  }

  /**
   * Set the OOSS context to attach to all events
   */
  setOOSSContext(context: OOSSContext): void {
    this.oossContext = context;
  }

  /**
   * Get the current OOSS context
   */
  getOOSSContext(): OOSSContext | undefined {
    return this.oossContext;
  }

  /**
   * Queue an event for streaming to Convex
   */
  queueEvent(event: UniversalEvent): void {
    // Enrich event with OOSS context
    const enrichedEvent: OOSSEnrichedEvent = {
      ...event,
      oossContext: this.oossContext,
    };

    // Check buffer size limit
    if (this.buffer.length >= this.config.maxBufferSize) {
      // Drop oldest events if buffer is full
      this.buffer.shift();
      console.warn('[ConvexEventStreamer] Buffer full, dropping oldest event');
    }

    this.buffer.push(enrichedEvent);

    // Flush immediately if batch is full
    if (this.buffer.length >= this.config.batchSize) {
      this.flush().catch((err) => {
        console.error('[ConvexEventStreamer] Flush error:', err);
      });
      return;
    }

    // Schedule flush if not already scheduled
    if (!this.flushTimeout) {
      this.flushTimeout = setTimeout(() => {
        this.flushTimeout = undefined;
        this.flush().catch((err) => {
          console.error('[ConvexEventStreamer] Scheduled flush error:', err);
        });
      }, this.config.flushIntervalMs);
    }
  }

  /**
   * Queue multiple events
   */
  queueEvents(events: UniversalEvent[]): void {
    for (const event of events) {
      this.queueEvent(event);
    }
  }

  /**
   * Flush buffered events to Convex
   */
  async flush(): Promise<void> {
    if (this.buffer.length === 0 || this.isFlushing) {
      return;
    }

    // Clear any pending flush timeout
    if (this.flushTimeout) {
      clearTimeout(this.flushTimeout);
      this.flushTimeout = undefined;
    }

    this.isFlushing = true;
    const eventsToSend = [...this.buffer];
    this.buffer = [];

    try {
      await this.sendToConvex(eventsToSend);
      this.isConnected = true;
    } catch (err) {
      this.isConnected = false;

      if (this.config.resilient) {
        // Put events back in buffer (at the front) for retry
        this.buffer = [...eventsToSend, ...this.buffer];
        // Trim if over limit
        while (this.buffer.length > this.config.maxBufferSize) {
          this.buffer.shift();
        }
        console.warn('[ConvexEventStreamer] Failed to send, will retry:', err);
      } else {
        throw err;
      }
    } finally {
      this.isFlushing = false;
    }
  }

  /**
   * Stop streaming and flush remaining events
   */
  async stop(): Promise<void> {
    if (this.flushTimeout) {
      clearTimeout(this.flushTimeout);
      this.flushTimeout = undefined;
    }

    // Final flush
    await this.flush();
  }

  /**
   * Get the number of buffered events
   */
  getBufferSize(): number {
    return this.buffer.length;
  }

  /**
   * Check if connected to Convex (last send succeeded)
   */
  isConvexConnected(): boolean {
    return this.isConnected;
  }

  /**
   * Send events to Convex mutation
   */
  private async sendToConvex(events: OOSSEnrichedEvent[]): Promise<void> {
    if (events.length === 0) return;

    await this.config.convexClient.mutation(this.config.eventMutation, {
      events,
    });
  }
}

/**
 * Create a Convex event streamer
 */
export function createConvexEventStreamer(
  config: ConvexEventStreamConfig
): ConvexEventStreamer {
  return new ConvexEventStreamer(config);
}

/**
 * Async generator wrapper that streams events to Convex while yielding them
 */
export async function* streamWithConvex(
  events: AsyncIterable<UniversalEvent>,
  streamer: ConvexEventStreamer
): AsyncGenerator<UniversalEvent, void, void> {
  try {
    for await (const event of events) {
      streamer.queueEvent(event);
      yield event;
    }
  } finally {
    // Ensure all events are flushed when stream ends
    await streamer.flush();
  }
}
