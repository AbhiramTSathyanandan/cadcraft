# web-embed — embed CADCraft in a page on another origin

`embed.html` is a static relay. It frames `/index.html?host=parent` (CADCraft's same-origin
postMessage bridge, see `apps/cadcraft-web/src/host.rs`) and forwards `cadcraft:*` messages
between that frame and an allow-listed parent page on a different origin:

- parent → CADCraft: `{ type: 'cadcraft:open', name, bytes }` (bytes `ArrayBuffer` or `Uint8Array`)
- CADCraft → parent: `cadcraft:ready`, `cadcraft:save { name, bytes }`, `cadcraft:dirty { dirty }`,
  `cadcraft:failed { error }`; plus `cadcraft:embed-ready` from the relay itself when it loads
  with `?parent=<origin>`.

Edit `ALLOWED` in the file to change which parent origins are accepted.

Deploy: copy it next to the web build and publish the directory —

```sh
trunk build --release   # in apps/cadcraft-web, or reuse dist/web
cp tools/web-embed/embed.html dist/web/
npx wrangler pages deploy dist/web --project-name craneye-cad-lab --branch main
```
