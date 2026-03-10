"use client";

import { useEffect, useState } from "react";
import {
  Card, CardBody, CardHeader,
  Input, Button, Switch,
} from "@heroui/react";
import type { AppConfig } from "@/lib/types";
import { clientApi } from "@/lib/client-api";

export default function SettingsPage() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    clientApi.getConfig().then(setConfig).catch(console.error);
  }, []);

  if (!config) {
    return <main className="p-6"><p className="text-default-500">Loading…</p></main>;
  }

  async function save() {
    if (!config) return;
    setSaving(true);
    try {
      await clientApi.putConfig(config);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="p-6 space-y-6 max-w-xl mx-auto">
      <h1 className="text-2xl font-bold">Settings</h1>

      <Card>
        <CardHeader><h2 className="font-semibold">Inverter</h2></CardHeader>
        <CardBody className="gap-4">
          <Input
            label="IP Address"
            value={config.inverter_ipv4_address}
            onValueChange={(v) => setConfig({ ...config, inverter_ipv4_address: v })}
          />
          <Input
            label="Poll interval (minutes)"
            type="number"
            value={String(config.polling_interval_minutes)}
            onValueChange={(v) => setConfig({ ...config, polling_interval_minutes: Number(v) })}
          />
        </CardBody>
      </Card>

      <Card>
        <CardHeader><h2 className="font-semibold">Tesla</h2></CardHeader>
        <CardBody className="gap-4">
          <Input
            label="VIN (optional)"
            value={config.tesla_vin ?? ""}
            onValueChange={(v) => setConfig({ ...config, tesla_vin: v || null })}
          />
          <Input
            label="Max charging amps"
            type="number"
            value={String(config.tesla_max_amps)}
            onValueChange={(v) => setConfig({ ...config, tesla_max_amps: Number(v) })}
          />
          <Switch
            isSelected={config.use_mock_ble}
            onValueChange={(v) => setConfig({ ...config, use_mock_ble: v })}
          >
            Use mock BLE (testing)
          </Switch>
        </CardBody>
      </Card>

      <Button color={saved ? "success" : "primary"} isLoading={saving} onPress={save}>
        {saved ? "Saved!" : "Save"}
      </Button>
    </main>
  );
}
