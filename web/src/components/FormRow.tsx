import FormField from "./FormField";

/** Inline label + control row (thin wrap around FormField). */
export default function FormRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <FormField label={label} layout="inline">
      {children}
    </FormField>
  );
}
