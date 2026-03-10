"use client";

import { useState } from "react";
import { Card, CardBody, CardHeader, Button, Chip, Slider } from "@heroui/react";
import type { TeslaState } from "@/lib/types";
import { clientApi } from "@/lib/client-api";

export function TeslaCard({ initialState }: { initialState: TeslaState | null }) {
  const [state, setState] = useState(initialState);
  const [loading, setLoading] = useState(false);
  const [amps, setAmps] = useState(8);

  async function run(action: () => Promise<void>) {
    setLoading(true);
    try {
      await action();
      // Refresh state after every action
      const updated = await clientApi.status();
      setState(updated.tesla);
    } finally {
      setLoading(false);
    }
  }

  return (
    <Card>
      <CardHeader className="flex justify-between">
        <h2 className="text-lg font-semibold">Tesla</h2>
        <Chip
          color={state?.connected ? "success" : "default"}
          variant="flat"
          size="sm"
        >
          {state?.connected ? "Connected" : "Disconnected"}
        </Chip>
      </CardHeader>
      <CardBody className="gap-4">
        {state && (
          <div className="flex gap-6 text-sm text-default-500">
            <span>Charging: <strong>{state.charging ? "Yes" : "No"}</strong></span>
            <span>Amps: <strong>{state.charging_amps} A</strong></span>
          </div>
        )}

        <div className="flex flex-wrap gap-2">
          <Button color="success" size="sm" isLoading={loading}
            onPress={() => run(clientApi.teslaStart)}>
            Start charging
          </Button>
          <Button color="danger" size="sm" isLoading={loading}
            onPress={() => run(clientApi.teslaStop)}>
            Stop charging
          </Button>
          <Button variant="bordered" size="sm" isLoading={loading}
            onPress={() => run(clientApi.teslaDiscover)}>
            Discover via BLE
          </Button>
        </div>

        <div className="max-w-sm">
          <p className="text-sm mb-2">Set amps: <strong>{amps} A</strong></p>
          <Slider
            minValue={1} maxValue={16} step={1}
            value={amps}
            onChange={(v) => setAmps(v as number)}
          />
          <Button className="mt-2" size="sm" isLoading={loading}
            onPress={() => run(() => clientApi.teslaSetAmps(amps))}>
            Apply
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}
