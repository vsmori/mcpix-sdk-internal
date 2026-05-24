// Demo .NET do mcpix-sdk — exercita o flow do recebedor:
//   1. Instancia o McpixReceiver
//   2. Registra um SeedId (gera Seed local random)
//   3. Gera uma cobrança → recebe campo de transporte público
//   4. Valida um C₂ deliberadamente errado → demonstra Mismatch
//
// Em produção, o C₂ correto chega via banco do pagador (HTTP mTLS),
// que reconstrói C₂ a partir do (Seed, T, C₁) lookup remoto. Este
// demo não exercita esse caminho — a SDK aqui é só a face do recebedor.

using MCPix.Sdk;

Console.WriteLine("=== mcpix-sdk — demo integrador .NET ===\n");

// (1) Instancia o receiver. Em produção este handle vive enquanto
// a aplicação roda; SeedStore/Counter/Rng internos são gerenciados
// pela impl default da SDK.
using var receiver = new McpixReceiver();

// (2) Registra um SeedId. A Seed é gerada localmente via OsRng
// (32 bytes de entropia criptográfica). Em produção, idealmente o
// material nasce no Secure Enclave / TPM — ver docs/SECURE_ELEMENT.md.
const string seedId = "RECVR1";
receiver.Register(seedId);
Console.WriteLine($"✓ recebedor cadastrado:  SeedId={seedId}");

// (3) Gera uma cobrança de R$ 99,00 (9900 centavos).
var (transportField, counter) = receiver.GenerateCharge(seedId, 9900);
Console.WriteLine($"✓ cobrança gerada:");
Console.WriteLine($"    transport field (público): {transportField}");
Console.WriteLine($"    counter T:                  {counter}");
Console.WriteLine($"    layout: PIXOFFv1 (8) + SeedId padded (16) + C₁ (11) = 35 chars");

// (4) Validate com C₂ inválido. Em produção, o pagador apresentaria
// o C₂ correto recuperado via banco; aqui usamos 11 'A's para
// exercitar o caminho Mismatch.
const string wrongC2 = "AAAAAAAAAAA"; // 11 chars; alfabeto válido mas
                                       // não bate com o retained no recebedor.
var outcome = receiver.ValidateReceipt(seedId, counter, wrongC2);
Console.WriteLine($"\n✓ validação com C₂ errado:");
Console.WriteLine($"    outcome: {outcome}  (esperado: Mismatch — demonstra defesa anti-tampering)");

if (outcome != McpixValidation.Mismatch)
{
    Console.Error.WriteLine($"  ✗ esperava Mismatch, recebeu {outcome}");
    return 1;
}

Console.WriteLine("\n--- demo completo. Próximos passos para integração real: ---");
Console.WriteLine("  • C₂ correto vem do banco do pagador via HTTP mTLS");
Console.WriteLine("    (lookup_seed → apply_recover_c2). Ver docs/PROTOCOL.md.");
Console.WriteLine("  • Persistência: SDK aceita SeedStore custom (SQLite, Keychain, etc).");
Console.WriteLine("  • Backup criptografado das sementes: crate mcpix-backup.");
return 0;
