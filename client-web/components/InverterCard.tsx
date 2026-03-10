import { Card, CardBody, CardHeader } from "@heroui/react";
import type { InverterData } from "@/lib/types";

export function InverterCard({ data }: { data: InverterData | null }) {
  if (!data) {
    return (
      <Card>
        <CardBody>
          <p className="text-default-500">No inverter data yet — waiting for first poll cycle.</p>
        </CardBody>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <h2 className="text-lg font-semibold">Solar Inverter</h2>
      </CardHeader>
      <CardBody>
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Metric label="Solar (PV)"  value={data.p_pv}   color="text-yellow-400" />
          <Metric label="House load"  value={data.p_load} color="text-blue-400"   />
          <Metric label="Grid"        value={data.p_grid} color="text-orange-400" />
          <Metric label="Battery"     value={data.p_akku} color="text-green-400"  />
        </div>
      </CardBody>
    </Card>
  );
}

function Metric({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div>
      <p className="text-sm text-default-500">{label}</p>
      <p className={`text-2xl font-mono font-bold ${color}`}>{value.toFixed(0)} W</p>
    </div>
  );
}
