/**
 * React hooks for Iridium editor integration.
 *
 * Uses the WASM/WebGPU browser API (not Node.js NAPI).
 */

import { useState, useEffect } from "react";

/**
 * WebGPU support status.
 */
export interface WebGPUStatus {
  supported: boolean;
  checked: boolean;
  adapterName?: string;
}

/**
 * Hook to check WebGPU support using native browser API.
 */
export function useWebGPUSupport(): WebGPUStatus {
  const [status, setStatus] = useState<WebGPUStatus>({
    supported: false,
    checked: false,
  });

  useEffect(() => {
    let mounted = true;

    async function check() {
      // Use native browser WebGPU check
      if (!navigator.gpu) {
        if (mounted) {
          setStatus({ supported: false, checked: true });
        }
        return;
      }

      try {
        const adapter = await navigator.gpu.requestAdapter();
        if (!mounted) return;

        if (adapter) {
          setStatus({
            supported: true,
            checked: true,
            adapterName: adapter.info?.device || "Unknown GPU",
          });
        } else {
          setStatus({ supported: false, checked: true });
        }
      } catch {
        if (mounted) {
          setStatus({ supported: false, checked: true });
        }
      }
    }

    check();

    return () => {
      mounted = false;
    };
  }, []);

  return status;
}
