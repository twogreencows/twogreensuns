// Server Component — fetches status on the Next.js server, passes data down.
import { serverApi } from "@/lib/server-api";
import { InverterCard } from "@/components/InverterCard";
import { TeslaCard } from "@/components/TeslaCard";

export const revalidate = 30; // re-fetch at most every 30 s on the server

export default async function DashboardPage() {
  const status = await serverApi.status().catch(() => null);

  return (
    <main className="p-6 space-y-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <InverterCard data={status?.inverter ?? null} />
      <TeslaCard initialState={status?.tesla ?? null} />
    </main>
  );
}
