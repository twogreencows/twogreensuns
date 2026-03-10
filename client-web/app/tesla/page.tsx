// Server Component — fetches initial Tesla state server-side, TeslaCard handles actions client-side.
import { serverApi } from "@/lib/server-api";
import { TeslaCard } from "@/components/TeslaCard";

export default async function TeslaPage() {
  const status = await serverApi.status().catch(() => null);

  return (
    <main className="p-6 space-y-6 max-w-2xl mx-auto">
      <h1 className="text-2xl font-bold">Tesla</h1>
      <TeslaCard initialState={status?.tesla ?? null} />
    </main>
  );
}
