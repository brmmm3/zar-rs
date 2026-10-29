namespace Zar;

/// <summary>Raised when czarlib reports a request, archive, or native-call failure.</summary>
public sealed class ZarException : Exception
{
    /// <summary>Native status code: 1 request/archive error, 2 invalid pointer or allocation, 3 Rust panic, 0 wrapper-side failure.</summary>
    public int Status { get; }

    public ZarException(int status, string message) : base(message)
    {
        Status = status;
    }
}
