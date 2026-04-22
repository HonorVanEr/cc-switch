// Polyfill for WebKitGTK 2.30 (JSC ≈ Safari 14)
// These APIs are used by dependencies but not available in older JSC engines.

// Object.hasOwn — ES2022, not available in Safari < 15.4
if (!(Object as any).hasOwn) {
  (Object as any).hasOwn = function (obj: object, prop: PropertyKey): boolean {
    return Object.prototype.hasOwnProperty.call(obj, prop);
  };
}

// Array.prototype.at — ES2022, not available in Safari < 15.4
if (!(Array.prototype as any).at) {
  (Array.prototype as any).at = function (index: number): unknown {
    const len = this.length;
    const i = index >= 0 ? index : len + index;
    return i >= 0 && i < len ? this[i] : undefined;
  };
}

// String.prototype.replaceAll — ES2021, not available in some older engines
if (!(String.prototype as any).replaceAll) {
  (String.prototype as any).replaceAll = function (pattern: string | RegExp, replacement: string): string {
    if (typeof pattern === "string") {
      return this.split(pattern).join(replacement);
    }
    if (pattern instanceof RegExp) {
      if (pattern.global) {
        return this.replace(pattern, replacement);
      }
      throw new TypeError(
        "String.prototype.replaceAll called with a non-global RegExp argument"
      );
    }
    throw new TypeError(
      "The first argument to String.prototype.replaceAll must be a string or a global RegExp"
    );
  };
}

// Promise.allSettled — ES2020, not available in very old engines
if (!(Promise as any).allSettled) {
  (Promise as any).allSettled = function (promises: Promise<unknown>[]): Promise<{ status: string; value?: unknown; reason?: unknown }[]> {
    return Promise.all(
      promises.map((p: Promise<unknown>) =>
        Promise.resolve(p).then(
          (value: unknown) => ({ status: "fulfilled", value }),
          (reason: unknown) => ({ status: "rejected", reason })
        )
      )
    );
  };
}
