// Wrapper P/Invoke idiomático sobre o C-ABI declarado em ../c/include/mcpix.h.
//
// Convenções:
// - PascalCase em métodos públicos, conforme .NET API design guidelines.
// - IDisposable: handle nativo é liberado explicitamente via mcpix_receiver_free.
// - Cada chamada nativa converte McpixStatus em McpixException quando != Ok.

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace MCPix.Sdk;

public enum McpixStatus
{
    Ok = 0,
    InvalidArgument = 1,
    TransportFieldLength = 2,
    TransportFieldCharset = 3,
    TransportFieldPrefix = 4,
    SeedIdLength = 5,
    SeedIdCharset = 6,
    SeedLength = 7,
    CounterOverflow = 8,
    UnknownSeed = 9,
    NoRetainedReceipt = 10,
    ReplayRejected = 11,
    Mismatch = 12,
    Storage = 13,
    Transport = 14,
    UnsupportedProtocolVersion = 15,
    Panic = 98,
    Unknown = 99,
}

public enum McpixValidation
{
    Valid = 0,
    Mismatch = 1,
    Replay = 2,
}

public sealed class McpixException : Exception
{
    public McpixStatus Status { get; }
    public McpixException(McpixStatus status, string message) : base(message)
    {
        Status = status;
    }
}

/// <summary>SDK do recebedor — wrapper P/Invoke sobre <c>mcpix_ffi</c>.</summary>
public sealed class McpixReceiver : IDisposable
{
    private const string LibName = "mcpix_ffi";

    private IntPtr _handle;

    public McpixReceiver()
    {
        var status = mcpix_receiver_new(out _handle);
        ThrowIfError(status);
    }

    public void Register(string seedId)
    {
        EnsureOpen();
        var status = mcpix_receiver_register(_handle, seedId);
        ThrowIfError(status);
    }

    public (string TransportField, ulong Counter) GenerateCharge(string seedId, ulong amountCents)
    {
        EnsureOpen();
        var status = mcpix_receiver_generate_charge(
            _handle, seedId, amountCents, out var fieldPtr, out var counter);
        ThrowIfError(status);
        try
        {
            return (Marshal.PtrToStringUTF8(fieldPtr)!, counter);
        }
        finally
        {
            mcpix_string_free(fieldPtr);
        }
    }

    public McpixValidation ValidateReceipt(string seedId, ulong counter, string presentedC2)
    {
        EnsureOpen();
        var status = mcpix_receiver_validate(_handle, seedId, counter, presentedC2, out var result);
        ThrowIfError(status);
        return (McpixValidation)result;
    }

    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            mcpix_receiver_free(_handle);
            _handle = IntPtr.Zero;
        }
        GC.SuppressFinalize(this);
    }

    ~McpixReceiver() => Dispose();

    private void EnsureOpen()
    {
        if (_handle == IntPtr.Zero)
            throw new ObjectDisposedException(nameof(McpixReceiver));
    }

    private static void ThrowIfError(McpixStatus status)
    {
        if (status != McpixStatus.Ok)
            throw new McpixException(status, $"native call failed: {status}");
    }

    // ── P/Invoke surface ───────────────────────────────────────────────────

    [DllImport(LibName, EntryPoint = "mcpix_receiver_new", CallingConvention = CallingConvention.Cdecl)]
    private static extern McpixStatus mcpix_receiver_new(out IntPtr handle);

    [DllImport(LibName, EntryPoint = "mcpix_receiver_free", CallingConvention = CallingConvention.Cdecl)]
    private static extern void mcpix_receiver_free(IntPtr handle);

    [DllImport(LibName, EntryPoint = "mcpix_receiver_register", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern McpixStatus mcpix_receiver_register(
        IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string seedId);

    [DllImport(LibName, EntryPoint = "mcpix_receiver_generate_charge", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern McpixStatus mcpix_receiver_generate_charge(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string seedId,
        ulong amountCents,
        out IntPtr outField,
        out ulong outCounter);

    [DllImport(LibName, EntryPoint = "mcpix_receiver_validate", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern McpixStatus mcpix_receiver_validate(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string seedId,
        ulong counter,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string presentedC2,
        out int outResult);

    [DllImport(LibName, EntryPoint = "mcpix_string_free", CallingConvention = CallingConvention.Cdecl)]
    private static extern void mcpix_string_free(IntPtr ptr);
}
