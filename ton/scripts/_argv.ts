// Tiny CLI arg helper shared by all test scripts. Parses `--key value` pairs
// from blueprint's `args: string[]` parameter.
//
// Underscore prefix keeps this file out of Blueprint's script discovery.

export function parseArgs(args: string[]): Record<string, string> {
    const out: Record<string, string> = {};
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        if (!a.startsWith('--')) continue;
        const key = a.slice(2);
        const val = args[i + 1];
        if (val === undefined || val.startsWith('--')) {
            out[key] = 'true';
        } else {
            out[key] = val;
            i++;
        }
    }
    return out;
}

export function mustArg(parsed: Record<string, string>, name: string): string {
    const v = parsed[name];
    if (v === undefined) {
        throw new Error(`missing --${name}`);
    }
    return v;
}
