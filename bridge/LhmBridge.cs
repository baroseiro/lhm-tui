// lhm-tui DLL bridge — loads LibreHardwareMonitorLib.dll directly and turns
// its sensor tree into the same JSON shape as LHM's /data.json endpoint.
//
// The same file is compiled twice by build.rs: with the .NET Framework csc.exe
// (classic LHM zip) and with the .NET 8+ SDK (LibreHardwareMonitor.NET.10.zip).
// Keep it C# 5 compatible so the Framework compiler stays happy.
// This file is original MIT-licensed lhm-tui code. LibreHardwareMonitorLib is
// a separate MPL-2.0 assembly loaded at runtime; no LHM source is copied here.
//
// Protocol (one request per line on stdin, one response per line on stdout):
//   POLL                  -> data.json-shaped JSON object
//   SET<TAB>id<TAB>value  -> {"result":"ok"} or {"result":"fail",...}
//   RESET                 -> {"result":"ok"} ...
//   QUIT                  -> {"result":"ok"}, then exit
// Errors are reported as a single `error <message>` line.
//
// Kept C# 5 compatible: the .NET Framework csc.exe that ships with Windows
// cannot compile newer language features.

using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Text;

internal static class LhmBridge
{
    private static Assembly assembly;
    private static Type computerType;
    private static object computer;
    private static string dllPath;
    private static string version = "unknown";

    private static readonly string[] SensorOrder = new string[] {
        "Voltage", "Current", "Power", "Clock", "Temperature", "Load",
        "Frequency", "Fan", "Flow", "Control", "Level", "Factor",
        "Data", "SmallData", "Throughput", "TimeSpan", "Timing",
        "Energy", "Noise", "Conductivity", "Humidity"
    };

    private static int Main(string[] args)
    {
        dllPath = null;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--dll" && i + 1 < args.Length)
            {
                dllPath = args[i + 1];
                i++;
            }
        }
        if (dllPath == null)
        {
            Console.Error.WriteLine("missing --dll <path>");
            return 2;
        }

        try
        {
            Console.InputEncoding = new UTF8Encoding(false);
            Console.OutputEncoding = new UTF8Encoding(false);
        }
        catch (Exception)
        {
            // Redirected pipes keep their default encoding; stdout is still
            // valid UTF-8 on every supported Windows build.
        }

        try
        {
            LoadLibrary();
        }
        catch (Exception ex)
        {
            Console.Out.WriteLine("error " + OneLine(LoadError(ex)));
            Console.Out.Flush();
            return 1;
        }

        Console.Out.WriteLine("ready " + version);
        Console.Out.Flush();

        try
        {
            string line;
            while ((line = Console.In.ReadLine()) != null)
            {
                line = line.Trim();
                if (line.Length == 0)
                {
                    continue;
                }
                if (line == "QUIT")
                {
                    CloseComputer();
                    Console.Out.WriteLine("{\"result\":\"ok\"}");
                    Console.Out.Flush();
                    return 0;
                }
                if (line == "POLL")
                {
                    Poll();
                }
                else if (line == "RESET")
                {
                    ResetMinMax();
                }
                else if (line.StartsWith("SET\t", StringComparison.Ordinal))
                {
                    SetControl(line);
                }
                else
                {
                    WriteError("unknown command");
                }
            }
        }
        catch (Exception ex)
        {
            WriteError(OneLine(ex.ToString()));
        }
        finally
        {
            CloseComputer();
        }
        return 0;
    }

    private static void LoadLibrary()
    {
        assembly = Assembly.LoadFrom(dllPath);
        computerType = assembly.GetType("LibreHardwareMonitor.Hardware.Computer", true);
        Version v = assembly.GetName().Version;
        if (v != null)
        {
            version = v.Major + "." + v.Minor + "." + v.Build;
        }
    }

    private static string LoadError(Exception ex)
    {
        string message = ex.Message;
        bool incompatible = ex is BadImageFormatException || ex is FileLoadException
            || message.IndexOf("newer than the currently loaded runtime", StringComparison.OrdinalIgnoreCase) >= 0
            || message.IndexOf("Bad IL format", StringComparison.OrdinalIgnoreCase) >= 0
            || message.IndexOf("System.Runtime, Version=1", StringComparison.OrdinalIgnoreCase) >= 0
            || message.IndexOf("System.Private.CoreLib", StringComparison.OrdinalIgnoreCase) >= 0;
        if (incompatible)
        {
            message += " — this assembly needs a newer .NET runtime; install the matching runtime "
                + "or use the .NET Framework LibreHardwareMonitor build";
        }
        return message;
    }

    private static void OpenComputer()
    {
        if (computer != null)
        {
            return;
        }
        object instance = Activator.CreateInstance(computerType);
        SetEnabled(instance, "IsCpuEnabled");
        SetEnabled(instance, "IsGpuEnabled");
        SetEnabled(instance, "IsMemoryEnabled");
        SetEnabled(instance, "IsMotherboardEnabled");
        SetEnabled(instance, "IsControllerEnabled");
        SetEnabled(instance, "IsNetworkEnabled");
        SetEnabled(instance, "IsStorageEnabled");
        SetEnabled(instance, "IsPsuEnabled");
        SetEnabled(instance, "IsBatteryEnabled");
        SetEnabled(instance, "IsPowerMonitorEnabled");
        computerType.GetMethod("Open", Type.EmptyTypes).Invoke(instance, null);
        computer = instance;
    }

    private static void SetEnabled(object instance, string property)
    {
        PropertyInfo info = computerType.GetProperty(property);
        if (info != null && info.PropertyType == typeof(bool) && info.CanWrite)
        {
            info.SetValue(instance, true, null);
        }
    }

    private static void CloseComputer()
    {
        if (computer == null)
        {
            return;
        }
        try
        {
            MethodInfo close = computerType.GetMethod("Close", Type.EmptyTypes);
            if (close != null)
            {
                close.Invoke(computer, null);
            }
        }
        catch (Exception)
        {
            // Best effort: never keep the bridge alive because Close failed.
        }
        computer = null;
    }

    private static void Poll()
    {
        try
        {
            OpenComputer();
            UpdateAll();
            StringBuilder json = new StringBuilder(256 * 1024);
            json.Append("{\"Version\":");
            JsonString(json, version);
            json.Append(",\"Children\":[");
            int id = 1;
            bool first = true;
            foreach (object hardware in AsEnumerable(Get(computer, "Hardware")))
            {
                if (!first)
                {
                    json.Append(',');
                }
                first = false;
                AppendHardware(json, hardware, ref id);
            }
            json.Append("]}");
            Console.Out.WriteLine(json.ToString());
            Console.Out.Flush();
        }
        catch (Exception ex)
        {
            WriteError(OneLine(ex.ToString()));
        }
    }

    private static void UpdateAll()
    {
        foreach (object hardware in AsEnumerable(Get(computer, "Hardware")))
        {
            UpdateHardware(hardware);
        }
    }

    private static void UpdateHardware(object hardware)
    {
        try
        {
            MethodInfo update = hardware.GetType().GetMethod("Update", Type.EmptyTypes);
            if (update != null)
            {
                update.Invoke(hardware, null);
            }
        }
        catch (Exception)
        {
            // One broken device must not hide every other sensor.
        }
        foreach (object sub in AsEnumerable(Get(hardware, "SubHardware")))
        {
            UpdateHardware(sub);
        }
    }

    private static void AppendHardware(StringBuilder json, object hardware, ref int id)
    {
        json.Append("{\"id\":").Append(id++);
        json.Append(",\"Text\":");
        JsonString(json, AsString(Get(hardware, "Name")));
        json.Append(",\"HardwareId\":");
        JsonString(json, AsString(Get(hardware, "Identifier")));
        json.Append(",\"ImageURL\":");
        JsonString(json, "images_icon/" + HardwareIcon(AsString(Get(hardware, "HardwareType"))));
        json.Append(",\"Children\":[");

        bool first = true;
        List<SensorItem> sensors = new List<SensorItem>();
        foreach (object sensor in AsEnumerable(Get(hardware, "Sensors")))
        {
            sensors.Add(new SensorItem(sensor, AsString(Get(sensor, "SensorType"))));
        }

        List<string> types = new List<string>();
        foreach (SensorItem item in sensors)
        {
            if (!types.Contains(item.Type))
            {
                types.Add(item.Type);
            }
        }
        types.Sort(CompareSensorTypes);

        foreach (string type in types)
        {
            if (!first)
            {
                json.Append(',');
            }
            first = false;
            json.Append("{\"id\":").Append(id++);
            json.Append(",\"Text\":");
            JsonString(json, GroupName(type));
            json.Append(",\"ImageURL\":");
            JsonString(json, "images_icon/" + GroupIcon(type));
            json.Append(",\"Children\":[");
            bool firstSensor = true;
            foreach (SensorItem item in sensors)
            {
                if (item.Type != type)
                {
                    continue;
                }
                if (!firstSensor)
                {
                    json.Append(',');
                }
                firstSensor = false;
                AppendSensor(json, item.Sensor, ref id);
            }
            json.Append("]}");
        }

        foreach (object sub in AsEnumerable(Get(hardware, "SubHardware")))
        {
            if (!first)
            {
                json.Append(',');
            }
            first = false;
            AppendHardware(json, sub, ref id);
        }

        json.Append("]}");
    }

    private static void AppendSensor(StringBuilder json, object sensor, ref int id)
    {
        json.Append("{\"id\":").Append(id++);
        json.Append(",\"Text\":");
        JsonString(json, AsString(Get(sensor, "Name")));
        json.Append(",\"SensorId\":");
        JsonString(json, AsString(Get(sensor, "Identifier")));
        json.Append(",\"Type\":");
        JsonString(json, AsString(Get(sensor, "SensorType")));
        json.Append(",\"RawMin\":").Append(Number(Get(sensor, "Min")));
        json.Append(",\"RawValue\":").Append(Number(Get(sensor, "Value")));
        json.Append(",\"RawMax\":").Append(Number(Get(sensor, "Max")));
        json.Append(",\"Children\":[]}");
    }

    private static void ResetMinMax()
    {
        try
        {
            OpenComputer();
            foreach (object hardware in AsEnumerable(Get(computer, "Hardware")))
            {
                ResetSensors(hardware);
            }
            Console.Out.WriteLine("{\"result\":\"ok\"}");
            Console.Out.Flush();
        }
        catch (Exception ex)
        {
            WriteError(OneLine(ex.ToString()));
        }
    }

    private static void ResetSensors(object hardware)
    {
        foreach (object sensor in AsEnumerable(Get(hardware, "Sensors")))
        {
            try
            {
                Invoke(sensor, "ResetMin");
                Invoke(sensor, "ResetMax");
            }
            catch (Exception)
            {
                // Keep resetting the remaining sensors.
            }
        }
        foreach (object sub in AsEnumerable(Get(hardware, "SubHardware")))
        {
            ResetSensors(sub);
        }
    }

    private static void SetControl(string line)
    {
        try
        {
            string[] parts = line.Split('\t');
            if (parts.Length != 3)
            {
                WriteError("SET needs an id and a value");
                return;
            }
            OpenComputer();
            object sensor = FindSensor(AsEnumerable(Get(computer, "Hardware")), parts[1]);
            if (sensor == null)
            {
                WriteError("unknown sensor id " + parts[1]);
                return;
            }
            object control = Get(sensor, "Control");
            if (control == null)
            {
                WriteError("sensor " + parts[1] + " is not writable");
                return;
            }
            if (parts[2] == "null")
            {
                control.GetType().GetMethod("SetDefault", Type.EmptyTypes).Invoke(control, null);
            }
            else
            {
                float value = float.Parse(parts[2], CultureInfo.InvariantCulture);
                if (float.IsNaN(value) || float.IsInfinity(value))
                {
                    WriteError("invalid control value " + parts[2]);
                    return;
                }
                control.GetType().GetMethod("SetSoftware", new Type[] { typeof(float) })
                    .Invoke(control, new object[] { value });
            }
            Console.Out.WriteLine("{\"result\":\"ok\"}");
            Console.Out.Flush();
        }
        catch (Exception ex)
        {
            WriteError(OneLine(ex.ToString()));
        }
    }

    private static object FindSensor(IEnumerable hardware, string id)
    {
        foreach (object item in hardware)
        {
            foreach (object sensor in AsEnumerable(Get(item, "Sensors")))
            {
                if (AsString(Get(sensor, "Identifier")) == id)
                {
                    return sensor;
                }
            }
            object found = FindSensor(AsEnumerable(Get(item, "SubHardware")), id);
            if (found != null)
            {
                return found;
            }
        }
        return null;
    }

    private static void Invoke(object target, string method)
    {
        MethodInfo info = target.GetType().GetMethod(method, Type.EmptyTypes);
        if (info != null)
        {
            info.Invoke(target, null);
        }
    }

    private static object Get(object target, string property)
    {
        if (target == null)
        {
            return null;
        }
        try
        {
            PropertyInfo info = target.GetType().GetProperty(property);
            return info == null ? null : info.GetValue(target, null);
        }
        catch (Exception)
        {
            return null;
        }
    }

    private static IEnumerable AsEnumerable(object value)
    {
        return value as IEnumerable ?? new object[0];
    }

    private static string AsString(object value)
    {
        return value == null ? "" : value.ToString();
    }

    private static string Number(object value)
    {
        if (value == null)
        {
            return "null";
        }
        float number;
        try
        {
            number = Convert.ToSingle(value, CultureInfo.InvariantCulture);
        }
        catch (Exception)
        {
            return "null";
        }
        if (float.IsNaN(number) || float.IsInfinity(number))
        {
            return "null";
        }
        return number.ToString("R", CultureInfo.InvariantCulture);
    }

    private static string HardwareIcon(string type)
    {
        switch (type)
        {
            case "Cpu": return "cpu.png";
            case "GpuNvidia": return "nvidia.png";
            case "GpuAmd": return "ati.png";
            case "GpuIntel": return "intel.png";
            case "Memory": return "ram.png";
            case "Storage": return "hdd.png";
            case "Network": return "nic.png";
            case "Motherboard": return "mainboard.png";
            case "SuperIO": return "chip.png";
            case "Cooler": return "cooler.png";
            case "EmbeddedController": return "embeddedcontroller.png";
            case "Psu": return "psu.png";
            case "Battery": return "battery.png";
            case "PowerMonitor": return "powermonitor.png";
            default: return "cpu.png";
        }
    }

    private static string GroupIcon(string type)
    {
        switch (type)
        {
            case "Temperature": return "temperature.png";
            case "Load": return "load.png";
            case "Clock":
            case "Frequency": return "clock.png";
            case "Voltage":
            case "Current":
            case "Conductivity": return "voltage.png";
            case "Power": return "power.png";
            case "Fan": return "fan.png";
            case "Control": return "control.png";
            case "Data":
            case "SmallData": return "data.png";
            case "Throughput": return "throughput.png";
            default: return "transparent.png";
        }
    }

    private static string GroupName(string type)
    {
        switch (type)
        {
            case "Temperature": return "Temperatures";
            case "Load": return "Load";
            case "Clock": return "Clocks";
            case "Voltage": return "Voltages";
            case "Current": return "Currents";
            case "Power": return "Powers";
            case "Fan": return "Fans";
            case "Control": return "Controls";
            case "Data":
            case "SmallData": return "Data";
            case "Throughput": return "Throughput";
            default: return type;
        }
    }

    private static int CompareSensorTypes(string left, string right)
    {
        int leftRank = Array.IndexOf(SensorOrder, left);
        int rightRank = Array.IndexOf(SensorOrder, right);
        if (leftRank < 0)
        {
            leftRank = SensorOrder.Length;
        }
        if (rightRank < 0)
        {
            rightRank = SensorOrder.Length;
        }
        if (leftRank != rightRank)
        {
            return leftRank.CompareTo(rightRank);
        }
        return string.CompareOrdinal(left, right);
    }

    private static void JsonString(StringBuilder json, string value)
    {
        json.Append('"');
        if (value != null)
        {
            foreach (char c in value)
            {
                switch (c)
                {
                    case '"': json.Append("\\\""); break;
                    case '\\': json.Append("\\\\"); break;
                    case '\b': json.Append("\\b"); break;
                    case '\f': json.Append("\\f"); break;
                    case '\n': json.Append("\\n"); break;
                    case '\r': json.Append("\\r"); break;
                    case '\t': json.Append("\\t"); break;
                    default:
                        if (c < ' ')
                        {
                            json.Append("\\u")
                                .Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        }
                        else
                        {
                            json.Append(c);
                        }
                        break;
                }
            }
        }
        json.Append('"');
    }

    private static string OneLine(string text)
    {
        return text.Replace('\r', ' ').Replace('\n', ' ').Replace('\t', ' ');
    }

    private static void WriteError(string message)
    {
        Console.Out.WriteLine("error " + OneLine(message));
        Console.Out.Flush();
    }

    private sealed class SensorItem
    {
        public readonly object Sensor;
        public readonly string Type;

        public SensorItem(object sensor, string type)
        {
            Sensor = sensor;
            Type = type;
        }
    }
}
