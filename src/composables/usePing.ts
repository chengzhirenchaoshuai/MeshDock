// 测速：单台测试和“全部测试”，结果只存在内存里
import { computed, ref, type Ref } from "vue";
import { ping, type CliError, type Device, type PingRecord } from "../api";

/** 设备的组网 IPv4，测试用；没有 IPv4 时退回第一个地址 */
export const deviceIp = (d: Device): string | undefined => d.ips.find((a) => !a.includes(":")) ?? d.ips[0];

/**
 * @param peers 当前设备列表
 * @param onDone 每次测试结束时调用（用于刷新“当前时间”，让 30 秒覆盖立即生效）
 */
export function usePing(peers: Ref<Device[]>, onDone: () => void) {
  /** 设备 id → 最近一次测试结果 */
  const pings = ref<Record<string, PingRecord>>({});
  /** 正在测试的设备 id */
  const testing = ref<Set<string>>(new Set());

  async function testOne(d: Device) {
    const ip = deviceIp(d);
    if (!ip || testing.value.has(d.id)) return;
    testing.value = new Set(testing.value).add(d.id);
    let record: PingRecord;
    try {
      record = { result: await ping(ip), error: null, atMs: Date.now() };
    } catch (e) {
      record = { result: null, error: e as CliError, atMs: Date.now() };
    }
    pings.value = { ...pings.value, [d.id]: record };
    onDone();
    const next = new Set(testing.value);
    next.delete(d.id);
    testing.value = next;
  }

  /** 在线设备才能测试，离线的跳过 */
  const testablePeers = computed(() => peers.value.filter((p) => p.conn.kind !== "offline"));
  const testingAll = computed(() => testing.value.size > 0);
  const testAll = () => Promise.all(testablePeers.value.map(testOne));

  return { pings, testing, testOne, testablePeers, testingAll, testAll };
}
