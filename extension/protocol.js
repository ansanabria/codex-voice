export const ACTIVE_STATES = Object.freeze(['recording', 'transcribing', 'typing']);

export function parseRuntimeStateText(text) {
    try {
        const value = JSON.parse(text);
        if (value?.schemaVersion !== 1 || !ACTIVE_STATES.includes(value.state)) return 'idle';
        if (!Number.isInteger(value.ownerPid) || value.ownerPid <= 0) return 'idle';
        if (!Number.isInteger(value.startedAt) || value.startedAt < 0) return 'idle';
        return value.state;
    } catch (_) {
        return 'idle';
    }
}
