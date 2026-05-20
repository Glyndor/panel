import { redirect } from "next/navigation";

export default async function MemberPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	redirect(`/${locale}/app/member/profile`);
}
