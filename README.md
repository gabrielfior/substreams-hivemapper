# substreams-hivemapper

### Generate protos
```bash
make protogen
```

### Build substreams
```bash
make build
```

### Set up token
Visit https://substreams.streamingfast.io/reference-and-specs/authentication to fetch a token or run below command if you have already followed the instructions from the linked documentation page.
```bash
sftoken
```

### Run substreams

~~substreams run substreams.yaml map_holders -e mainnet.sol.streamingfast.io:443 -t +1000~~

Following the module_name from substreams.yaml, I executed this command:
```bash
substreams run substreams.yaml map_outputs -e mainnet.sol.streamingfast.io:443 -t +1000
```
But received the error below:
```
Connected (trace ID e3db7830a2071cad757f6db326baeff5)
Progress messages received: 0 (0/sec)
Backprocessing history up to requested target block 154656000:
(hit 'm' to switch mode)


Error: rpc error: code = Unknown desc = rpc error: code = Internal desc = step new irr: handler step new: execute modules: applying executor results "map_outputs": execute: maps wasm call: block 154656004: module "map_outputs": general wasm execution failed: call: expected 2 params, but passed 3
```