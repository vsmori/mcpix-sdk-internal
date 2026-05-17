# **ESPECIFICAÇÃO TÉCNICA: ARQUITETURA DE SDK MULTIPLATAFORMA EM RUST**

## **Bloco 1: O Núcleo Isolado (Rust Core)**

O coração do SDK será escrito em Rust puro (código altamente estável, tipado e imutável). Sua principal premissa é o **isolamento total de I/O** (Entrada/Saída). O Core processa dados e dita o protocolo; ele não gerencia conexões diretamente.

### **1.1 Gerenciamento de Estado e Imutabilidade**

* Todas as estruturas de dados internas devem ser imutáveis por padrão.  
* Modificações de estado devem seguir o padrão de transição de estado funcional: f(EstadoAtual, Comando) \-\> NovoEstado.  
* O gerenciamento de memória deve se apoiar estritamente no sistema de *ownership* do Rust, evitando alocações desnecessárias no *heap*.

### **1.2 Abstração de Efeitos Colaterais (Traits)**

O Core não pode instanciar clientes HTTP, ler arquivos do disco ou acessar chaves de hardware diretamente. Ele define contratos (*Traits*) que as plataformas nativas devem implementar e injetar.

Rust

// Exemplo estrutural de abstração de rede dentro do Core Rust  
pub trait HttpTransport: Send \+ Sync {  
    fn send\_request(&self, request: RawRequest) \-\> Result\<RawResponse, TransportError\>;  
}

// O Core recebe a implementação por Injeção de Dependência  
pub struct FinancialSdkCore {  
    transport: Box\<dyn HttpTransport\>,  
}

### **1.3 Política de Não-Pânico (*Panic Safety*)**

* O SDK **nunca** deve capotar (*panic*) a aplicação hospedeira.  
* Todas as funções públicas expostas na FFI devem ser protegidas por std::panic::catch\_unwind.  
* Erros internos devem ser mapeados em um enum robusto e convertidos em códigos de erro numéricos ou strings estruturadas para o ambiente externo.

## ---

**Bloco 2: A Camada FFI e Geração de Bindings**

Para que o código Rust seja lido por outras linguagens, ele precisa expor uma interface compatível com C (extern "C") ou utilizar geradores de interface estruturada. A abordagem padrão para este projeto será o uso do **UniFFI** (ferramenta que automatiza a geração de pontes para Swift e Kotlin) e assinaturas C nativas para o ecossistema .NET.

\[ Camada Externa: Swift / Kotlin / C\# \]  
                   │  
                   ▼ (Chamada Idiomática)  
\[ Camada de Bindings Gerada (UniFFI / P-Invoke) \]  
                   │  
                   ▼ (Tipos Primitivos C / C-ABI)  
\[ FFI Bridge (extern "C" em Rust) \]  
                   │  
                   ▼  
\[   Rust Core (Lógica e Segurança)   \]

### **2.1 Mapeamento de Tipos na Fronteira (C-ABI)**

* Dados complexos cruzam a fronteira FFI serializados em formatos eficientes (ex: Protocol Buffers ou BSON em memória) ou mapeados via estruturas C estruturadas (\#\[repr(C)\]).  
* Strings devem ser tratadas com cuidado: o Rust recebe \*const c\_char e deve convertê-las com segurança usando CStr.

## ---

**Bloco 3: Fachadas Idiomáticas (Consumo nas Plataformas)**

Cada linguagem que consumir o SDK receberá um pacote que encapsula a complexidade da FFI, expondo uma interface perfeitamente alinhada com os padrões modernos de cada plataforma.

### **3.1 Camada iOS (Swift)**

O pacote final será distribuído como um *Swift Package*. A fachada oculta os ponteiros C e expõe concorrência moderna com async/await e tratamento de erros nativo com Result ou throws.

* **Padrão de Nomenclatura:** CamelCase, seguindo estritamente as *Swift API Design Guidelines*.  
* **Injeção de I/O:** O Swift passa uma instância que usa URLSession para o Rust.

Swift

// Exemplo de como o desenvolvedor iOS consumirá o SDK  
import FinancialSdk

public class FinancialSdkFacade {  
    private let corePointer: OpaquePointer  
      
    public init(transport: HttpTransportDelegate) {  
        // Inicializa o core passando o delegate nativo do iOS (URLSession)  
        self.corePointer \= financial\_sdk\_init(transport)  
    }  
      
    public func executeTransaction(payload: TransactionData) async throws \-\> TransactionResult {  
        return try await withCheckedThrowingContinuation { continuation in  
            financial\_sdk\_execute(self.corePointer, payload) { result in  
                switch result {  
                case .success(let data): continuation.resume(returning: data)  
                case .failure(let error): continuation.resume(throwing: error)  
                }  
            }  
        }  
    }  
}

### **3.2 Camada Android & Java Backend (Kotlin / Java JVM)**

No Android e no Java Backend, o SDK será distribuído como um arquivo .aar ou .jar. A ponte usa JNI (via UniFFI) para carregar a biblioteca nativa (.so ou .dylib).

* **Padrão de Nomenclatura:** camelCase, suporte a tipos reativos ou assíncronos nativos da JVM.  
* **Concorrência Android:** Exposição de funções suspensas (suspend) utilizando Kotlin Coroutines.  
* **Concorrência Java Backend:** Exposição via CompletableFuture.

Kotlin

// Exemplo de consumo idiomático no Android / Kotlin  
package com.financial.sdk

import kotlinx.coroutines.Dispatchers  
import kotlinx.coroutines.withContext

class FinancialSdkNative(private val config: SdkConfig) {  
    private val nativeContext: Long \= initializeNativeCore(config)

    // Expõe uma função suspensa idiomática para o desenvolvedor Android  
    async fun processPayment(request: PaymentRequest): PaymentResponse \= withContext(Dispatchers.Default) {  
        val rawResult \= nativeProcessPayment(nativeContext, request)  
        if (rawResult.isSuccess) {  
            return@withContext rawResult.toResponse()  
        } else {  
            throw FinancialSdkException(rawResult.errorMessage)  
        }  
    }  
      
    private external fun initializeNativeCore(config: SdkConfig): Long  
    private external fun nativeProcessPayment(context: Long, req: PaymentRequest): RawResult  
}

### **3.3 Camada .NET Backend (C\#)**

No backend C\#, a integração acontece por meio do mecanismo de **P/Invoke** (DllImport ou o moderno LibraryImport do .NET 7+) para carregar a DLL/SO de forma direta e extremamente veloz.

* **Padrão de Nomenclatura:** PascalCase, propriedades automáticas e contratos assíncronos baseados em Task.  
* **Gerenciamento de Recursos:** A classe fachada deve implementar IDisposable para liberar ponteiros alocados no lado do Rust de forma explícita.

C\#

// Exemplo de consumo idiomático no C\# (.NET)  
using System;  
using System.Runtime.InteropServices;  
using System.Threading.Tasks;

namespace Financial.Sdk  
{  
    public class FinancialSdkWrapper : IDisposable  
    {  
        private IntPtr \_coreHandle;

        // Importação nativa da DLL gerada pelo Rust  
        \[DllImport("financial\_sdk\_core", EntryPoint \= "sdk\_init")\]  
        private static extern IntPtr SdkInit();

        \[DllImport("financial\_sdk\_core", EntryPoint \= "sdk\_free")\]  
        private static extern void SdkFree(IntPtr handle);

        public FinancialSdkWrapper()  
        {  
            \_coreHandle \= SdkInit();  
        }

        // Fachada que expõe o padrão Task (TAP) do C\#  
        public Task\<ValidationResult\> ValidateSecurePayloadAsync(string payload)  
        {  
            return Task.Run(() \=\>  
            {  
                // Executa a chamada nativa via ponte P/Invoke  
                var resultPointer \= NativeValidate(\_coreHandle, payload);  
                return Marshal.PtrToStructure\<ValidationResult\>(resultPointer);  
            });  
        }

        \[DllImport("financial\_sdk\_core", EntryPoint \= "sdk\_validate")\]  
        private static extern IntPtr NativeValidate(IntPtr handle, \[MarshalAs(UnmanagedType.LPStr)\] string payload);

        public void Dispose()  
        {  
            if (\_coreHandle \!= IntPtr.Zero)  
            {  
                SdkFree(\_coreHandle);  
                \_coreHandle \= IntPtr.Zero;  
            }  
            GC.SuppressFinalize(this);  
        }  
    }  
}

## ---

**Bloco 4: Pipeline de Geração e Distribuição (CI/CD)**

Como se trata de um código nativo, o processo de build precisa gerar binários para cada arquitetura alvo (cross-compilation).

### **4.1 Alvos de Compilação (Targets)**

O pipeline automatizado (via GitHub Actions ou GitLab CI) deve compilar o Core Rust utilizando o rustup target add para os seguintes alvos:

| Ambiente Alvo | Arquiteturas Necessárias | Formato de Saída do Rust |
| :---- | :---- | :---- |
| **iOS (Físico & Simulador)** | aarch64-apple-ios, aarch64-apple-ios-sim | .a (Static Library) empacotada em um XCFramework |
| **Android (Arm & x86)** | aarch64-linux-android, armv7-linux-androideabi, x86\_64-linux-android | .so (Dynamic Library) injetada dentro do arquivo .aar |
| **Java Backend (Linux/Windows)** | x86\_64-unknown-linux-gnu, x86\_64-pc-windows-msvc | .so / .dll incluídos no classpath do .jar |
| **C\# Backend (.NET)** | x86\_64-unknown-linux-gnu, x86\_64-pc-windows-msvc | .so / .dll distribuídos via pacote NuGet interno |

### **4.2 Validação de Integridade**

Por operar no mercado financeiro, a esteira de build deve injetar uma assinatura digital e gerar hashes SHA-256 para cada artefato compilado. Qualquer alteração ou injeção de código binário na cadeia de custódia invalidará o SDK imediatamente na inicialização.