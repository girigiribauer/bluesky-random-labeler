/**
 * 指定された日時 (または現在日時) を日本時間 (JST) の "YYYY-MM-DD" 形式の文字列で返します。
 * @param date 対象の日時 (省略時は現在日時)
 * @returns "YYYY-MM-DD" 形式の文字列
 */
export function getJstDate(date?: Date): string {
    const targetDate = date ?? new Date();
    return new Intl.DateTimeFormat("ja-JP", {
        timeZone: "Asia/Tokyo",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    })
        .format(targetDate)
        .replace(/\//g, "-");
}

/**
 * 指定された日時 (または現在日時) を日本時間 (JST) の "YYYY-MM-DD HH:mm:ss" 形式の文字列で返します。
 * @param date 対象の日時 (省略時は現在日時)
 * @returns "YYYY-MM-DD HH:mm:ss" 形式の文字列
 */
export function getJstTime(date?: Date): string {
    const targetDate = date ?? new Date();
    return new Intl.DateTimeFormat("ja-JP", {
        timeZone: "Asia/Tokyo",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    })
        .format(targetDate)
        .replace(/\//g, "-");
}
