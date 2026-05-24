# Sample .NET — consumer da MCPix.Sdk via NuGet

Demo console minimalista mostrando o flow do recebedor: cadastrar
semente, gerar cobrança, validar comprovante. Replica o que
`examples/e2e_demo.rs` faz no Rust puro, mas atravessando a fronteira
P/Invoke via o pacote `MCPix.Sdk`.

## Pré-requisitos

- .NET 8 SDK
- `libmcpix_ffi.so` (Linux) ou `mcpix_ffi.dll` (Windows) compilados via:
  ```bash
  cargo xtask build-linux    # ou build-windows
  ```

  O `MCPixSDK.csproj` espera que esses binários estejam em
  `dist/<platform>/` ao empacotar via NuGet. Para `dotnet run`
  direto a partir deste sample, defina `LD_LIBRARY_PATH` para que
  o runtime ache a `.so`:

  ```bash
  cargo xtask build-linux
  cd examples/dotnet-sample
  LD_LIBRARY_PATH=../../dist/linux-x86_64 dotnet run
  ```

## Build & run

```bash
dotnet build
LD_LIBRARY_PATH=../../dist/linux-x86_64 dotnet run
```

Saída esperada:

```
=== mcpix-sdk — demo integrador .NET ===

✓ recebedor cadastrado:  SeedId=RECVR1
✓ cobrança gerada:
    transport field (público): PIXOFFv1RECVR10000000000XXXXXXXXXXX
    counter T:                  1
    layout: PIXOFFv1 (8) + SeedId padded (16) + C₁ (11) = 35 chars

✓ validação com C₂ errado:
    outcome: Mismatch  (esperado: Mismatch — demonstra defesa anti-tampering)

--- demo completo. Próximos passos para integração real: ---
  • C₂ correto vem do banco do pagador via HTTP mTLS
    (lookup_seed → apply_recover_c2). Ver docs/PROTOCOL.md.
  • Persistência: SDK aceita SeedStore custom (SQLite, Keychain, etc).
  • Backup criptografado das sementes: crate mcpix-backup.
```

## O que esse sample NÃO mostra

- **C₂ válido para passar como `Valid`** — em produção, o C₂ correto
  vem do banco do pagador via HTTP mTLS (que recupera Seed do
  recebedor e roda `apply_recover_c2`). A SDK do recebedor só expõe
  `register/generateCharge/validateReceipt`; o caminho do pagador
  fica fora do binding por design.
- **Persistência cross-restart** — o `McpixReceiver` default usa
  store em memória. Para persistir, implemente `SeedStore` no Rust
  (ver `mcpix-receiver-sdk::sqlite_store`) e exponha via UniFFI.
- **mTLS inter-bancos** — separado, em `mcpix-bank-receiver`.

Esses caminhos completos estão em [`../e2e_demo.rs`](../e2e_demo.rs)
(Rust host, sem fronteira P/Invoke).
