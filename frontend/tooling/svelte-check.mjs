// Load TS6 first so svelte-check's compiler API peer cannot bind to the CLI-only TS7 package.
await import("npm:@typescript/typescript6@6.0.2");
await import("npm:svelte-check@4.7.2/bin/svelte-check");
