export default function FormRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-3 flex items-center gap-3">
      <label className="w-[140px] shrink-0 text-[0.875rem] font-medium text-text">{label}</label>
      <div className="flex-1">{children}</div>
    </div>
  );
}
