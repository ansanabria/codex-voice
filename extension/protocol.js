export const ACTIVE_STATES = Object.freeze(['recording', 'transcribing', 'typing']);

export function parseRuntimeStateText(text) {
    try {
        const value = JSON.parse(text);
        if (value?.schemaVersion !== 1 || !ACTIVE_STATES.includes(value.state)) return null;
        if (!Number.isInteger(value.ownerPid) || value.ownerPid <= 0) return null;
        if (!Number.isSafeInteger(value.ownerStartTime) || value.ownerStartTime <= 0) return null;
        if (!Number.isSafeInteger(value.startedAt) || value.startedAt < 0) return null;
        return {
            state: value.state,
            ownerPid: value.ownerPid,
            ownerStartTime: value.ownerStartTime,
        };
    } catch (_) {
        return null;
    }
}

export function processStartTimeFromStat(text) {
    const closingParen = text.lastIndexOf(')');
    if (closingParen < 0) return null;

    // Fields after the command name begin at field 3; start time is field 22.
    const fields = text.slice(closingParen + 1).trim().split(/\s+/);
    const startTime = fields[19];
    return /^\d+$/.test(startTime ?? '') ? startTime : null;
}

export function runtimeStateOwnerIsCurrent(runtimeState, statText) {
    return processStartTimeFromStat(statText) === String(runtimeState.ownerStartTime);
}
