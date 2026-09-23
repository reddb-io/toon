/**
 * The two conversions the `toon` CLI performs, over the canonical event-stream
 * codec: JSON to TOON and TOON back to JSON. Results go to stdout or to
 * `--output`; every diagnostic goes to stderr, so a pipeline stays clean.
 */
import * as path from 'node:path';
import { decodeStream } from '../decode/stream.js';
import { encode, encodeLines } from '../encode/serialize.js';
import { CliError } from './errors.js';
import { formatInputLabel, readInput, readLinesFromSource, writeStream, } from './io.js';
import { jsonStreamFromEvents } from './json-from-events.js';
import { formatStatistics } from './tokens.js';
export async function encodeToToon(config) {
    const { io } = config;
    const jsonContent = await readInput(config.input, io);
    const data = parseJson(jsonContent);
    const encodeOptions = { delimiter: config.delimiter, indentSize: config.indentSize };
    if (!config.shouldPrintStats) {
        await writeStream(encodeLines(data, encodeOptions), {
            outputPath: config.output,
            separator: '\n',
            io,
        });
        reportWritten('Encoded', config);
        return;
    }
    // Token counting needs the whole document, so the streaming form buys nothing here.
    const toonOutput = encode(data, encodeOptions);
    await writeStream([toonOutput], { outputPath: config.output, separator: '', io });
    reportWritten('Encoded', config);
    const statistics = formatStatistics(jsonContent, toonOutput);
    io.stderr(`● ${statistics.estimates}\n`);
    io.stderr(`✔ ${statistics.saved}\n`);
}
export async function decodeToJson(config) {
    const { io } = config;
    const lineSource = readLinesFromSource(config.input, config.strict, io);
    const events = decodeStream(lineSource, {
        indentSize: config.indentSize,
        strict: config.strict,
    });
    await writeStream(jsonStreamFromEvents(events, config.indentSize), {
        outputPath: config.output,
        separator: '',
        io,
    });
    reportWritten('Decoded', config);
}
/**
 * Validates the input without producing a result: stdout stays empty and the
 * verdict goes to stderr, so `toon --check` gates a pipeline on its exit code.
 * TOON is decoded in full; JSON is parsed and encoded, proving it is TOON-able.
 */
export async function checkInput(config) {
    const { io } = config;
    const label = formatInputLabel(config.input, io);
    if (config.mode === 'decode') {
        const lineSource = readLinesFromSource(config.input, config.strict, io);
        const events = decodeStream(lineSource, { indentSize: config.indentSize, strict: config.strict });
        for await (const _event of events) {
            // Draining the stream is the validation; nothing is kept.
        }
        io.stderr(`✔ Valid TOON \`${label}\`\n`);
        return;
    }
    const data = parseJson(await readInput(config.input, io));
    for (const _line of encodeLines(data, { delimiter: config.delimiter, indentSize: config.indentSize })) {
        // Encoding proves the value fits TOON; the lines are discarded.
    }
    io.stderr(`✔ Valid JSON \`${label}\`\n`);
}
function parseJson(jsonContent) {
    try {
        return JSON.parse(jsonContent);
    }
    catch (error) {
        throw new CliError(`Failed to parse JSON: ${error instanceof Error ? error.message : String(error)}`, { cause: error });
    }
}
function reportWritten(verb, config) {
    if (!config.output)
        return;
    const inputLabel = formatInputLabel(config.input, config.io);
    const outputLabel = path.relative(config.io.cwd, config.output);
    config.io.stderr(`✔ ${verb} \`${inputLabel}\` → \`${outputLabel}\`\n`);
}
