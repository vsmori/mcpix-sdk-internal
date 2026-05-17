import uniffi.mcpix_uniffi.McpixReceiver
import uniffi.mcpix_uniffi.McpixValidation
import uniffi.mcpix_uniffi.McpixUniffiException
import kotlin.test.Test
import kotlin.test.assertTrue
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class SmokeTest {

    @Test
    fun `full receiver flow round-trips through native lib`() {
        McpixReceiver().use { sdk ->
            sdk.register("R1")
            val charge = sdk.generateCharge("R1", 9900u)

            assertTrue(charge.transportField.startsWith("PIXOFFv1"))
            assertEquals(35, charge.transportField.length)
            assertTrue(charge.counter > 0u)

            // Como o smoke test não dispõe do mock do banco do pagador via
            // binding, validamos negativamente: um C₂ aleatório de tamanho
            // certo deve dar Mismatch, não Valid nem exceção.
            val outcome = sdk.validateReceipt("R1", charge.counter, "AAAAAAAAAAA")
            assertEquals(McpixValidation.MISMATCH, outcome)
        }
    }

    @Test
    fun `invalid seed id is a typed exception`() {
        McpixReceiver().use { sdk ->
            assertFailsWith<McpixUniffiException.InvalidSeedId> {
                sdk.register("R0")  // '0' é proibido no alfabeto do SeedId
            }
        }
    }
}
