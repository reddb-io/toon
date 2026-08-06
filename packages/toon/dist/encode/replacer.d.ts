export type EncodeReplacer = (key: string, value: any, path: readonly (string | number)[]) => unknown;
/** Applies the JSON-style replacer before shape detection and emission. */
export declare function applyReplacer(root: any, replacer: EncodeReplacer): any;
