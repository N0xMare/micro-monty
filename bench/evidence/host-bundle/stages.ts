/**
 * The bundle guest against the default guest: session memory by stage and first-import
 * times on the JS host, shared by node.ts (Node) and worker.ts (workerd).
 */
import { Runtime, type Session, sessionOptions } from "../../../hosts/js/src/host.ts";

export const CELLS: Record<string, string> = {
	cell: "x = 1 + 2\nprint(x)\n",
	imports: "import json, re, collections\ny = json.dumps({'a': [1, 2]})\n",
};

export class Stages {
	readonly rt: Runtime;
	live: Session[] = [];
	constructor(module: WebAssembly.Module) {
		this.rt = Runtime.fromModule(module);
	}
	add(n: number): void {
		for (let i = 0; i < n; i++) {
			const s = this.rt.session(sessionOptions({}));
			s.setOutput(() => {});
			this.live.push(s);
		}
	}
	run(code: string): void {
		for (const s of this.live) check(s.start(code), code);
	}
	clear(): void {
		for (const s of this.live) s.dispose();
		this.live = [];
	}
	/** `reps` fresh sessions, each running `x = 1` then `import <m>` when `m` is set (workerd: timed outside). */
	batch(m: string, reps: number): void {
		for (let i = 0; i < reps; i++) {
			const s = this.rt.session(sessionOptions({}));
			s.setOutput(() => {});
			check(s.start("x = 1"), "x = 1");
			if (m) check(s.start(`import ${m}`), m);
			s.dispose();
		}
	}
	/** Medians in µs: a new session, its first cell, then the first import of each module in a fresh session. */
	times(reps: number): Record<string, number> {
		const med = (v: number[]) => v.sort((a, b) => a - b)[v.length >> 1] * 1000;
		const r: Record<string, number> = {};
		const fresh = (): Session => {
			const s = this.rt.session(sessionOptions({}));
			s.setOutput(() => {});
			return s;
		};
		const nw: number[] = [];
		const first: number[] = [];
		for (let i = 0; i < reps; i++) {
			let t = performance.now();
			const s = fresh();
			nw.push(performance.now() - t);
			t = performance.now();
			check(s.start("x = 1"), "x = 1");
			first.push(performance.now() - t);
			s.dispose();
		}
		r.session_new = med(nw);
		r.first_cell = med(first);
		for (const m of ["json", "re", "collections", "datetime"]) {
			const v: number[] = [];
			for (let i = 0; i < reps; i++) {
				const s = fresh();
				check(s.start("x = 1"), "x = 1");
				const t = performance.now();
				check(s.start(`import ${m}`), m);
				v.push(performance.now() - t);
				s.dispose();
			}
			r[`import_${m}`] = med(v);
		}
		return r;
	}
}

function check(p: { kind: string }, code: string): void {
	if (p.kind !== "complete") throw new Error(`${code}: ${JSON.stringify(p)}`);
}
