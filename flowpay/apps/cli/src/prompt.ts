import {createInterface} from "node:readline/promises";
import {Writable} from "node:stream";

export async function prompt(label: string, secret = false): Promise<string> {
  if (!process.stdin.isTTY) throw new Error("Interactive login requires a terminal; use device approval or --code instead");
  let muted = false;
  const output = new Writable({write(chunk, _encoding, done) {
    if (!muted) process.stderr.write(chunk);
    done();
  }});
  const rl = createInterface({input: process.stdin, output, terminal: true});
  try {
    process.stderr.write(label);
    muted = secret;
    return await rl.question("");
  } finally {
    rl.close();
    output.end();
    if (secret) process.stderr.write("\n");
  }
}
