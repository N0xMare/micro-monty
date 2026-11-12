// The few workerd declarations worker.ts needs, so the type check needs no
// @cloudflare/workers-types (as pi's cloudflare-modules.d.ts does).
declare module "cloudflare:workers" {
	export abstract class DurableObject<Env = unknown> {
		protected ctx: DurableObjectState;
		protected env: Env;
		constructor(ctx: DurableObjectState, env: Env);
	}
}
declare module "*.wasm" {
	const module: WebAssembly.Module;
	export default module;
}
declare module "pi-codemode" {
	export const InlineCodemodeSandbox: unknown;
}
interface SqlCursor<T> {
	one(): T;
	toArray(): T[];
}
interface DurableObjectState {
	storage: { sql: { exec<T = Record<string, unknown>>(query: string, ...bindings: unknown[]): SqlCursor<T> } };
}
interface DurableObjectStub {
	fetch(req: Request): Promise<Response>;
}
interface DurableObjectNamespace {
	idFromName(name: string): unknown;
	get(id: unknown): DurableObjectStub;
}
