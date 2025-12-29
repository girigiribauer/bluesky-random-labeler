import { createHash } from "node:crypto";

export const FORTUNES = [
    { val: "daikichi", threshold: 6 },   // 6%
    { val: "kichi", threshold: 28 },     // 22%
    { val: "chukichi", threshold: 50 },  // 22%
    { val: "shokichi", threshold: 70 },  // 20%
    { val: "suekichi", threshold: 88 },  // 18%
    { val: "kyo", threshold: 97 },       // 9%
    { val: "daikyo", threshold: 100 },   // 3%
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
