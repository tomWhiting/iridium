/**
 * The part of the `iridium-bindings` wasm module this package calls.
 *
 * The module is a peerDependency that wasm-pack builds from
 * `crates/iridium-bindings/src/wasm.rs`, so it is not present to type-check
 * against. These are the four exports the controller uses, written as
 * wasm-bindgen declares them. The editor `createWebEditor` resolves to is
 * typed by the controller's own `WebEditor` interface at the call site.
 */
declare module "iridium-bindings" {
  /** Fetches and instantiates the wasm module. */
  export default function init(): Promise<unknown>;
  /** The device pixel ratio the kernel will actually use. */
  export function sanitizePixelRatio(ratio: number): number;
  /** The host command ids, as a JSON object. */
  export function hostCommandIds(): string;
  /** Builds the editor on `canvas`; rejects when the GPU cannot start. */
  export function createWebEditor(canvas: HTMLCanvasElement, pixelRatio: number): Promise<unknown>;
}
