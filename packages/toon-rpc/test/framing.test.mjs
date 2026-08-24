import assert from 'node:assert/strict';
import { test } from 'node:test';
import { FrameDecoder, FramingError, encodeFrame } from '../dist/framing.js';

const encoder = new TextEncoder();
const text = (document) => new TextDecoder('utf8', { fatal: true }).decode(document);

test('a frame round-trips one complete document', () => {
  const decoder = new FrameDecoder();
  const documents = decoder.push(encodeFrame(encoder.encode('toonrpc: "1.0"\nresult: 2\nid: 1')));
  assert.equal(documents.length, 1);
  assert.equal(text(documents[0]), 'toonrpc: "1.0"\nresult: 2\nid: 1');
  assert.equal(decoder.hasPartialFrame, false);
});

test('documents containing blank lines and length-like text survive framing', () => {
  const tricky = 'first\n\n\n12\nsecond\n\n';
  const decoder = new FrameDecoder();
  const documents = decoder.push(encodeFrame(encoder.encode(tricky)));
  assert.equal(documents.length, 1);
  assert.equal(text(documents[0]), tricky);
});

test('an empty document is a valid frame', () => {
  const decoder = new FrameDecoder();
  const documents = decoder.push(encodeFrame(new Uint8Array(0)));
  assert.equal(documents.length, 1);
  assert.equal(documents[0].length, 0);
});

test('a frame split across arbitrary chunks reassembles', () => {
  const frame = encodeFrame(encoder.encode('a: 1\nb: 2'));
  for (let split = 1; split < frame.length; split += 1) {
    const decoder = new FrameDecoder();
    assert.deepEqual(decoder.push(frame.slice(0, split)), []);
    const documents = decoder.push(frame.slice(split));
    assert.equal(documents.length, 1, `split at ${split}`);
    assert.equal(text(documents[0]), 'a: 1\nb: 2');
  }
});

test('multiple frames in one chunk decode in order', () => {
  const chunk = new Uint8Array([
    ...encodeFrame(encoder.encode('one')),
    ...encodeFrame(encoder.encode('two')),
    ...encodeFrame(encoder.encode('three')),
  ]);
  const decoder = new FrameDecoder();
  const documents = decoder.push(chunk);
  assert.deepEqual(documents.map(text), ['one', 'two', 'three']);
});

test('byte-by-byte delivery of several frames works', () => {
  const stream = new Uint8Array([
    ...encodeFrame(encoder.encode('x')),
    ...encodeFrame(encoder.encode('yz')),
  ]);
  const decoder = new FrameDecoder();
  const documents = [];
  for (const byte of stream) documents.push(...decoder.push(new Uint8Array([byte])));
  assert.deepEqual(documents.map(text), ['x', 'yz']);
});

test('a non-decimal length fails the stream', () => {
  const decoder = new FrameDecoder();
  assert.throws(() => decoder.push(encoder.encode('12a\npayload')), FramingError);
  assert.throws(() => decoder.push(new Uint8Array([1])), FramingError);
});

test('a negative or padded length fails the stream', () => {
  assert.throws(() => new FrameDecoder().push(encoder.encode('-1\n')), FramingError);
  assert.throws(() => new FrameDecoder().push(encoder.encode('01\nx\n')), FramingError);
  assert.throws(() => new FrameDecoder().push(encoder.encode('\nx')), FramingError);
});

test('an unterminated or oversized length header fails the stream', () => {
  assert.throws(() => new FrameDecoder().push(encoder.encode('1234567890123456')), FramingError);
  assert.throws(
    () => new FrameDecoder().push(encoder.encode('1234567890123456\nx')),
    FramingError
  );
});

test('a payload without its terminator fails the stream', () => {
  const decoder = new FrameDecoder();
  assert.throws(() => decoder.push(encoder.encode('2\nabX')), FramingError);
});

test('finish rejects a stream that ends inside a frame', () => {
  const decoder = new FrameDecoder();
  decoder.push(encoder.encode('5\nab'));
  assert.throws(() => decoder.finish(), FramingError);
  const clean = new FrameDecoder();
  clean.push(encodeFrame(encoder.encode('done')));
  clean.finish();
});
