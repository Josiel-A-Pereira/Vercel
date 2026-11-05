using System.Windows.Forms;

namespace AgentDeskFixtureApp
{
    /// <summary>
    /// A text box that writes, for every navigation key it receives, whether
    /// the key arrived marked extended (bit 24 of the WM_KEYDOWN lParam) to a
    /// literal-value readout: "down extended" for the dedicated arrow,
    /// "down keypad" for its numeric keypad twin. A stock Edit control moves
    /// and selects the same either way (probe A31-2), so the bit is the only
    /// thing a press leg can observe to tell the two keys apart.
    /// </summary>
    internal sealed class KeyReportingTextBox : TextBox
    {
        private const int WM_KEYDOWN = 0x0100;

        public TextBox Report;

        protected override void WndProc(ref Message m)
        {
            if (m.Msg == WM_KEYDOWN && Report != null)
            {
                string name = NavigationKeyName(m.WParam.ToInt64());
                if (name != null)
                {
                    bool extended = ((m.LParam.ToInt64() >> 24) & 1) == 1;
                    Report.Text = name + (extended ? " extended" : " keypad");
                }
            }
            base.WndProc(ref m);
        }

        private static string NavigationKeyName(long virtualKey)
        {
            switch (virtualKey)
            {
                case 0x21: return "pageup";
                case 0x22: return "pagedown";
                case 0x23: return "end";
                case 0x24: return "home";
                case 0x25: return "left";
                case 0x26: return "up";
                case 0x27: return "right";
                case 0x28: return "down";
                case 0x2E: return "forwarddelete";
                default: return null;
            }
        }
    }
}
