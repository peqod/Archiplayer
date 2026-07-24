/**
 * Generation token for async UI work. Starting a newer request or invalidating
 * the current one makes older completions stale without needing transport-level
 * cancellation support.
 */
export class LatestRequest {
  private generation = 0;

  begin(): number {
    this.generation += 1;
    return this.generation;
  }

  isCurrent(generation: number): boolean {
    return generation === this.generation;
  }

  invalidate(generation?: number): void {
    if (generation === undefined || this.isCurrent(generation)) {
      this.generation += 1;
    }
  }
}
