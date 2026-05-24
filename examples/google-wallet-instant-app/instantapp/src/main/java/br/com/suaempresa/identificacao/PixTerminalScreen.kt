package br.com.suaempresa.identificacao

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import uniffi.mcpix_uniffi.McpixCharge

/**
 * Terminal Pix em Jetpack Compose (spec §4: "Interface Fluida"). Fluxo:
 *   loading (fetch init) → input do valor → gerar OFFLINE → QR.
 */
@Composable
fun PixTerminalScreen(memberId: String) {
    val generator = remember { PixGenerator() }
    var ready by remember { mutableStateOf(false) }
    var errorText by remember { mutableStateOf<String?>(null) }
    var amountText by remember { mutableStateOf("") }
    var charge by remember { mutableStateOf<McpixCharge?>(null) }

    // Init: bootstrap do gerador (fetch + restore da Seed).
    LaunchedEffect(memberId) {
        if (memberId.isBlank()) {
            errorText = "Link sem parâmetro ?id="
            return@LaunchedEffect
        }
        runCatching {
            // Em produção: passphrase via BiometricPrompt + Keystore.
            generator.bootstrap(memberId, passphrase = "demo-passphrase")
        }.onSuccess { ready = true }
            .onFailure { errorText = "Falha ao inicializar: ${it.message}" }
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Recebimento Pix", style = MaterialTheme.typography.headlineSmall)

        when {
            errorText != null -> {
                Text(errorText!!, color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center)
            }

            !ready -> {
                CircularProgressIndicator()
                Text("Validando chave $memberId…")
            }

            charge != null -> ResultView(charge!!) {
                charge = null
                amountText = ""
            }

            else -> {
                Text("Recebedor: ${generator.holderName}", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(
                    value = amountText,
                    onValueChange = { amountText = it },
                    label = { Text("Valor (R$)") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                    singleLine = true,
                )
                Button(
                    onClick = {
                        errorText = null
                        val cents = parseCents(amountText)
                        if (cents == null) {
                            errorText = "Valor inválido"
                        } else {
                            runCatching { generator.generate(cents) }
                                .onSuccess { charge = it }
                                .onFailure { errorText = "Falha ao gerar: ${it.message}" }
                        }
                    },
                    enabled = parseCents(amountText) != null,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Gerar QR Code") }
                Text(
                    "Gerado offline — sem chamada ao PSP por transação.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

@Composable
private fun ResultView(charge: McpixCharge, onNew: () -> Unit) {
    val bmp = remember(charge.transportField) { generateQrBitmap(charge.transportField) }
    Image(bitmap = bmp.asImageBitmap(), contentDescription = "QR Pix offline",
        modifier = Modifier.size(240.dp))
    Text(charge.transportField, style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
    Text("T = ${charge.counter}  ·  PIXOFFv1  ·  offline", style = MaterialTheme.typography.labelSmall)
    OutlinedButton(onClick = onNew) { Text("Nova cobrança") }
}

/** "12,34" ou "12.34" → 1234 centavos (ULong). */
private fun parseCents(text: String): ULong? {
    val v = text.replace(",", ".").toDoubleOrNull() ?: return null
    if (v <= 0) return null
    return (v * 100).toULong()
}
