// WebGPU type declarations for navigator.gpu
interface GPUAdapterInfo {
  vendor?: string;
  architecture?: string;
  device?: string;
  description?: string;
}

interface GPUAdapter {
  readonly info?: GPUAdapterInfo;
  readonly name?: string;
}

interface GPU {
  requestAdapter(options?: GPURequestAdapterOptions): Promise<GPUAdapter | null>;
}

interface GPURequestAdapterOptions {
  powerPreference?: "low-power" | "high-performance";
}

interface Navigator {
  gpu?: GPU;
}
