import { createHash } from "node:crypto";

export const FORTUNES = [
    { val: "daikichi", label: "大吉", threshold: 6 },   // 6%
    { val: "kichi", label: "吉", threshold: 28 },     // 22%
    { val: "chukichi", label: "中吉", threshold: 50 },  // 22%
    { val: "shokichi", label: "小吉", threshold: 70 },  // 20%
    { val: "suekichi", label: "末吉", threshold: 88 },  // 18%
    { val: "kyo", label: "凶", threshold: 97 },       // 9%
    { val: "daikyo", label: "大凶", threshold: 100 },   // 3%
];

export function getDailyFortune(did: string, date: Date = new Date()): string {
    const jstNow = new Date(date.toLocaleString("en-US", { timeZone: "Asia/Tokyo" }));
    const dateStr = jstNow.toISOString().split("T")[0];
    const seed = did + dateStr;
    const hash = createHash("sha256").update(seed).digest();
    const val = hash.readUInt32BE(0) % 100;

    for (const fortune of FORTUNES) {
        if (val < fortune.threshold) {
            return fortune.val;
        }
    }
    return "kichi";
}
