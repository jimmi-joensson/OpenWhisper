using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

namespace OpenWhisper.Tray;

/// <summary>
/// Procedurally draws the OpenWhisper mic glyph as an <see cref="Icon"/> suitable for
/// the Windows system tray. Mirrors <c>apps/macos/App/OpenWhisperApp.swift</c>'s
/// <c>StatusIconRenderer</c> — same 26-rect mic shape on a 792×792 viewBox, same
/// idle-vs-recording fill treatment. Keeping both platforms off the same rect
/// list means when the icon shape changes the SVG/rect source gets updated once
/// and both shells are re-ported.
/// </summary>
internal static class StatusIconRenderer
{
    private const float ViewBox = 792f;

    // Rect list copied verbatim from `OpenWhisperApp.swift:242-269`. Any
    // change to the mic glyph should sync both files.
    private static readonly RectangleF[] MicRects =
    {
        new(204, 188, 64, 64),
        new(204, 284, 64, 64),
        new(204, 380, 64, 64),
        new(204, 476, 64, 64),
        new(204, 700, 64, 64),
        new(268, 28, 64, 64),
        new(268, 92, 256, 64),
        new(268, 188, 64, 64),
        new(268, 284, 64, 64),
        new(268, 380, 64, 64),
        new(268, 476, 256, 64),
        new(268, 700, 256, 64),
        new(364, 28, 64, 64),
        new(364, 156, 64, 320),
        new(364, 572, 64, 64),
        new(364, 636, 64, 64),
        new(460, 28, 64, 64),
        new(460, 188, 64, 64),
        new(460, 284, 64, 64),
        new(460, 380, 64, 64),
        new(524, 92, 64, 64),
        new(524, 188, 64, 64),
        new(524, 284, 64, 64),
        new(524, 380, 64, 64),
        new(524, 476, 64, 64),
        new(524, 700, 64, 64),
    };

    public static Icon RenderIdle(int size) => Render(size, Color.FromArgb(235, 235, 235));

    public static Icon RenderRecording(int size) => Render(size, Color.FromArgb(224, 112, 0)); // #E07000

    private static Icon Render(int size, Color fill)
    {
        using var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp))
        {
            g.Clear(Color.Transparent);
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;

            float scale = size / ViewBox;
            using var brush = new SolidBrush(fill);
            foreach (var r in MicRects)
            {
                g.FillRectangle(
                    brush,
                    r.X * scale,
                    r.Y * scale,
                    r.Width * scale,
                    r.Height * scale);
            }
        }

        // Bitmap.GetHicon() returns an HICON we own and must destroy. Wrap it in
        // an Icon that clones the handle internally so we can DestroyIcon the
        // original — otherwise every icon render leaks a GDI object.
        IntPtr hIcon = bmp.GetHicon();
        try
        {
            using var fromHandle = Icon.FromHandle(hIcon);
            return (Icon)fromHandle.Clone();
        }
        finally
        {
            DestroyIcon(hIcon);
        }
    }

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyIcon(IntPtr hIcon);
}
