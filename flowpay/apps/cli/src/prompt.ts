import {createInterface} from "node:readline/promises";
import {Writable} from "node:stream";

export async function prompt(label: string, secret = false): Promise<string> {
  if (!process.stdin.isTTY) throw new Error("This step requires a terminal; provide registration flags and --code for scripts");
  const output = new Writable({write(chunk, _encoding, done) {
    if (!secret) process.stderr.write(chunk);
    done();
  }});
  const rl = createInterface({input: process.stdin, output, terminal: true});
  try {
    if(secret)process.stderr.write(label);
    return await rl.question(secret?"":label);
  } finally {
    rl.close();
    output.end();
    if (secret) process.stderr.write("\n");
  }
}
