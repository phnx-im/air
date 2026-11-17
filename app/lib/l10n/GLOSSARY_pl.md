# Glosariusz lokalizacji (polski)

Ten glosariusz ustala podstawowe terminy interfejsu Air, aby tłumaczenia pozostały spójne.

Szablon `app_en.arb` określa, co mówi dany tekst. Glosariusz określa, jakimi terminami to mówi. Jeśli szablon i glosariusz są ze sobą sprzeczne, zgłoś konflikt zamiast samodzielnie wybierać.

## Wariant

Air używa ogólnej polszczyzny i zasad pisowni Wielkiego słownika ortograficznego PWN. Kod tłumaczenia to `pl`, a selektor języka pokazuje „Polski”.

Wariant językowy dostaje własne pliki, własny plik ARB i własny glosariusz, i nie miesza się z językiem podstawowym.

## Zwracanie się do użytkownika

Air zwraca się do użytkownika na „ty” we wszystkich tekstach, tak jak Android i Apple. Forma nie zmienia się między tekstami.

Zaimki odnoszące się do użytkownika piszemy wielką literą, także w środku zdania: Ty, Ciebie, Cię, Tobie, Ci, Tobą, Twój, Twoja, Twoje, Twojego, Twoim i pozostałe formy. Tak robi Android. Zaimek zwrotny „swój” piszemy małą literą. Zaimki odnoszące się do innych osób i do rzeczy też piszemy małą literą.

Przyciski i pozycje menu są w trybie rozkazującym w 2. osobie liczby pojedynczej: „Usuń”, „Wyślij”, „Anuluj”, „Spróbuj ponownie”. Instrukcje i komunikaty o błędach też: „Spróbuj ponownie.”, „Wpisz kod”.

Tytuł potwierdzenia w formie pytania to bezokolicznik ze znakiem zapytania: „Usunąć wiadomość?”, „Zablokować użytkownika {displayName}?”.

Stałe sformułowania:

- „Are you sure you want to…” to „Czy na pewno chcesz …?”.
- „Failed to…” i „Couldn't…” to „Nie udało się …”: „Nie udało się wysłać wiadomości. Spróbuj ponownie.”
- „Something went wrong” to „Coś poszło nie tak”.
- Trwająca czynność to rzeczownik odczasownikowy: „Wysyłanie…”, „Łączenie…”.
- Jeśli angielski tekst nie zawiera „please”, tłumaczenie nie dodaje „proszę”.

## Rodzaj gramatyczny

Aplikacja nie zna płci użytkownika ani osób, których imiona trafiają do symboli zastępczych. Dlatego:

- Czas przeszły innej osoby ma formę z „(a)”, tak jak w tłumaczeniu rosyjskim: „{user1} dodał(a) użytkownika {user2}”, „{displayName} zaakceptował(a) Twoją prośbę o kontakt.” Gdy forma żeńska zmienia temat, końcówka stoi w nawiasie z łącznikiem: „{user} usunął(-ęła) czat”. Forma z „(a)” dotyczy tylko osób, nigdy rzeczy.
- Czas przeszły użytkownika nie ma form „usunąłeś”, „usunęłaś” ani „usunąłeś(-aś)”. Zamiast nich stosuj formę bezosobową na „-no” i „-to” albo stronę bierną z „przez Ciebie”: „Wysłano prośbę o kontakt do użytkownika {displayName}.”, „Ta wiadomość została przez Ciebie usunięta.”, „Z Twoim kontem połączono nowe urządzenie {device}.”
- Strona bierna innej osoby to forma bezosobowa z rzeczownikiem ogólnym: „Dodano użytkownika {user1} do czatu”, „Usunięto użytkownika {user1} z czatu”. Nie „{user1} został(a) dodany(-a)”.
- Czas przyszły złożony buduje się z bezokolicznikiem, który nie ma rodzaju: „będziesz otrzymywać”, nie „będziesz otrzymywał”.
- Bez przymiotników i imiesłowów o użytkowniku: „Czy na pewno chcesz zamknąć?”, nie „Jesteś pewien?”.
- Bez rzeczowników męskich o użytkowniku: „Nie należysz już do tego czatu.”, nie „Nie jesteś już członkiem tego czatu.”
- Tryb przypuszczający („mógłbyś”) ma rodzaj. Używaj „możesz” albo „można”.
- Zaimki „on”, „ona”, „go” i „ją” zdradzają płeć. Angielskie „they” i „them” o jednej osobie to „ta osoba”, „tę osobę” albo powtórzone imię: „dopóki nie odblokujesz tej osoby”.

## Odmiana i symbole zastępcze

Aplikacja nie odmienia imion ani nazw w symbolach zastępczych. Dlatego:

- Jako podmiot imię stoi w mianowniku bez dodatków: „{displayName} wysłał(a) Ci prośbę o kontakt.”
- Gdy imię powinno stać w innym przypadku, przypadek przejmuje rzeczownik ogólny przed symbolem zastępczym, a imię się nie zmienia: „użytkownika {displayName}”, „od użytkownika {displayName}”, „z użytkownikiem {displayName}”, „w grupie {groupName}”, „z grupy {groupName}”, „do czatu {chatName}”, „urządzenie {device}”, „nazwę użytkownika {username}”.
- Rzeczownik ogólny to domyślnie „użytkownik”. Gdy angielski tekst mówi „contact”, jest nim „kontakt”: „Usunąć kontakt {displayName}?”. Przed nazwą grupy stoi „grupa”, przed nazwą czatu „czat”, przed nazwą urządzenia „urządzenie”, a przed nazwą użytkownika „nazwa użytkownika”.
- Do symbolu zastępczego nie dopisujemy końcówek, ani bezpośrednio, ani po apostrofie lub łączniku. Nie piszemy „{displayName}a”, „{displayName}'a” ani „{displayName}-owi”.
- Przyimek nie stoi bezpośrednio przed imieniem ani nazwą. Między nimi zawsze jest rzeczownik ogólny. Wyjątkiem są liczby, daty i godziny: „za {count} dni”, „{date} o {time}”.

## Wielkie litery

Obowiązuje polska ortografia, nie angielska. W tytułach i na przyciskach wielką literą piszemy tylko pierwszy wyraz i nazwy własne, nawet jeśli angielski szablon pisze wielką literą każdy wyraz: „Kody zaproszenia”, „Kod bezpieczeństwa”. Nazwy dni, miesięcy i języków piszemy małą literą: „poniedziałek”, „angielski”. Tytuły dokumentów, takie jak „Warunki korzystania”, mają wielką literę tylko w pierwszym wyrazie. Zaimki odnoszące się do użytkownika piszemy wielką literą, jak opisano wyżej.

## Interpunkcja

Cudzysłów to „…”, z dolnym znakiem otwierającym i górnym zamykającym. Cudzysłów wewnętrzny to «…». Angielskie "..." i “...” zamieniamy w tłumaczeniu na „...”. Wyjątkiem jest `deleteAccountScreen_confirmationInputLabel`, gdzie słowo 'delete' zostaje nieprzetłumaczone w prostych apostrofach, bo aplikacja porównuje dokładnie to słowo.

Nazwa przycisku, karty lub opcji w zdaniu stoi w cudzysłowie, nawet jeśli szablon go nie ma: dotknij „Połącz urządzenie”.

Przecinek stoi przed zdaniem podrzędnym i przed spójnikami takimi jak „że”, „który”, „aby”, „jeśli”, „gdy” i „dopóki”, także gdy angielskie zdanie nie ma przecinka: „Upewnij się, że kod jest poprawny.”

Tekst kończy się tym samym znakiem co oryginał. Może to być kropka, znak zapytania, wykrzyknik lub dwukropek. Jeśli oryginał nie ma znaku na końcu, tłumaczenie też go nie ma. Wielokropek odpowiada zapisowi z szablonu. Jeśli szablon ma trzy kropki „...”, tłumaczenie też je ma. Jeśli ma jeden znak „…”, tłumaczenie też go ma.

Separator w ścieżce menu pozostaje taki sam jak w szablonie, na przykład „>” lub „→”.

## Liczby i jednostki

Między liczbą a jednostką stoi spacja: „5 MB”. Jednostki bajtów to B, kB, MB, GB, TB, PB, EB, ZB i YB, z małym „k”. Separatorem dziesiętnym jest przecinek, który formatuje aplikacja.

Znacznik czasu dla minut to „{count} min”, bez kropki.

## Liczba mnoga

Polski używa kategorii CLDR `one`, `few`, `many` i `other`. Dla liczb całkowitych `one` obejmuje tylko 1 („1 urządzenie”). `few` obejmuje liczby kończące się na 2, 3 lub 4, z wyjątkiem 12, 13 i 14 („3 urządzenia”). `many` obejmuje pozostałe liczby całkowite, w tym 0 („5 urządzeń”). Gałąź `other` obejmuje ułamki i jest obowiązkowa, bez niej kontrola nie przechodzi. W gałęzi `other` stosujemy dopełniacz liczby pojedynczej, jak przy ułamkach: „1,5 urządzenia”. Gałąź `=0` formułujemy bez liczby, jeśli brzmi to lepiej: „Brak połączonych urządzeń.”

Jeśli tekst jest taki sam we wszystkich formach, wystarczy jedna gałąź `other`.

## Air

„Air” nie jest tłumaczone i się nie odmienia, tak jak nazwy linii lotniczych w rodzaju „Air France”: „w Air”, „do Air”, „z Air”. Nie dopisujemy końcówek, czyli nie „Aira” ani „Airze”. Gdy nieodmieniona nazwa brzmi niezręcznie, pomaga rzeczownik „aplikacja”: „w aplikacji Air”. „Air account” to „konto Air”, „Air contact” to „kontakt Air”, „on Air” to „w Air”, „Join Air” to „Dołącz do Air”, a „Contact Air” to „Skontaktuj się z Air”.

## Podstawowe terminy

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Air** | Nazwa aplikacji | Nie tłumaczymy i nie odmieniamy |
| **Konto** | Konto użytkownika w Air | „konto Air” |
| **Nazwa użytkownika** | Unikalna nazwa, którą się udostępnia, aby inni mogli nawiązać kontakt | Tylko do nawiązywania kontaktu. Liczba mnoga „nazwy użytkownika” |
| **Nazwa wyświetlana** | Nazwa, którą inni widzą w czatach | Ang. „display name”. Coś innego niż nazwa użytkownika. Nie „pseudonim” ani „nazwa profilu” |
| **Czat** | Rozmowa dwóch lub więcej osób | Ang. „chat”. Liczba mnoga „czaty”. Nie „rozmowa” |
| **Kontakt** | Inny użytkownik, do którego możesz pisać | „kontakt Air” |
| **Członek** | Uczestnik czatu grupowego | W grupach. Także o kobietach. Nie w zwrotach do użytkownika |
| **Osoba, osoby** | Ang. „person, people” | „Dodaj osoby”, „5 osób”. Także dla „they” i „them” o jednej osobie |
| **Użytkownik** | Każdy, kto korzysta z Air | Domyślny rzeczownik ogólny przed symbolem zastępczym |
| **Aplikacja** | Ang. „app” | Nie „apka” |

## Wiadomości

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Wiadomość** | Tekst, obraz lub plik wysłany w czacie | |
| **Usuń u mnie**, **Usuń u wszystkich** | Usunięcie wiadomości tylko z własnych urządzeń albo u wszystkich w czacie | |
| **Wersja robocza** | Napisana, ale niewysłana wiadomość | Na liście czatów |
| **Załącznik** | Plik lub obraz wysłany z wiadomością | |
| **Edytuj** | Zmiana już wysłanej wiadomości | Oznaczenie na wiadomości „Edytowano”. Ang. „change” to „Zmień”: „Zmień nazwę grupy” |
| **Odpowiedz** | Odpowiedź na konkretną wiadomość | |
| **Zareaguj** | Dodanie emoji do wiadomości | W tekście o innej osobie „zareagował(a)” |
| **Emoji** | Znak obrazkowy | Rodzaj nijaki, nieodmienne: „to emoji”, „te emoji” |
| **Grupa** | Czat z wieloma członkami | Sam czat to „czat grupowy” |

## Działania i interfejs

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Prośba o kontakt** | Prośba wysłana przez nazwę użytkownika lub przez grupę | Ang. „contact request”. Ang. „connect” o ludziach to „nawiązać kontakt”. Nigdy „połączyć” |
| **Dodaj** | Dodanie kogoś do grupy lub do kontaktów | |
| **Usuń** | Ang. „remove” i „delete” | Jeden czasownik dla obu. Różnicę wskazuje dopełnienie: „Usuń z grupy”, „Usuń wiadomość”. Nie „wyrzuć” ani „skasuj” |
| **Opuść** | Samodzielne wyjście z grupy | „Opuść grupę”. Coś innego niż usunięcie, które dotyczy innej osoby |
| **Zablokuj** | Wstrzymanie wiadomości od kogoś | |
| **Odblokuj** | Cofnięcie blokady | Ten sam wyraz na przyciskach i w treści okien dialogowych |
| **Wycisz** | Wyłączenie powiadomień czatu | |
| **Wyłącz wyciszenie** | Cofnięcie wyciszenia | Zawsze to wyrażenie |
| **Połącz** | Danie innemu urządzeniu dostępu do konta | Tylko o urządzeniach. „Połączone urządzenia” |
| **Odłącz** | Odebranie dostępu połączonemu urządzeniu | |
| **Zaakceptuj** | Przyjęcie prośby o kontakt | |
| **Odrzuć** | Odrzucenie prośby o kontakt | Nadawca nie dostaje powiadomienia |
| **Wycofaj** | Cofnięcie wysłanej prośby o kontakt | „Wycofaj prośbę” |
| **Zgłoś spam** | Oznaczenie kogoś jako nadawcy niechcianych wiadomości | Moderacja |
| **Dotknij** | Ang. „tap” | Telefony i tablety. Także w tekście o innym urządzeniu |
| **Kliknij** | Ang. „click” | Komputer, klucze `_desktop` |
| **Link** | Ang. „link” do adresu internetowego | „Otworzyć link?”. Nie mylić z „połącz”, które dotyczy urządzeń |

## Pliki i dane

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Jednostki bajtów** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Spacja między liczbą a jednostką |
| **Prześlij** | Wysłanie pliku na serwer | Ang. „upload”. Rzeczownik „przesyłanie” |
| **Obrazy** | Wybór obrazu z galerii | „Aparat” do zrobienia nowego zdjęcia |

## Status i czas

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Teraz** | W tej chwili | Znacznik czasu bardzo nowej wiadomości |
| **{count} min** | Minut temu | Krótko, bez kropki |
| **Dzisiaj**, **Wczoraj** | Bieżący i poprzedni dzień | Separatory dat na liście wiadomości |
| **Wysyłanie** | Wiadomość jest w drodze na serwer | Status wiadomości |
| **Nie wysłano** | Nie udało się wysłać wiadomości | Status wiadomości |
| **Wysłano** | Wiadomość dotarła na serwer | Status wiadomości. Statusy mają formę bezosobową |
| **Dostarczono** | Wiadomość dotarła na urządzenie odbiorcy | Status wiadomości |
| **Przeczytano** | Odbiorca zobaczył wiadomość | Status wiadomości |
| **Potwierdzenia odczytu** | Ustawienie, które decyduje, czy status odczytu jest udostępniany | |
| **Wygasa** | Wygaśnięcie prośby o kontakt | „Wygasa za 3 dni”, „Prośba o kontakt wygasła.” |

## Ustawienia i pomoc

| Termin | Znaczenie | Kontekst |
|--------|-----------|----------|
| **Profil i ustawienia** | Ekran ustawień | |
| **Ty** | Ang. „You”, karta | Także na liście członków i przy reakcjach |
| **Notatki dla siebie** | Czat z notatkami między własnymi urządzeniami | Ang. „Notes to self” |
| **Kod bezpieczeństwa** | Kod, który porównują dwa kontakty, aby upewnić się, że nikt nie podsłuchuje czatu | |
| **Szyfrowanie end-to-end** | Ang. „end-to-end encryption” | Przymiotnik „szyfrowane end-to-end” |
| **Kod zaproszenia** | Kod potrzebny, aby dołączyć do Air | Zawsze „kod zaproszenia” |
| **Urządzenia** | Ang. „Devices”, wiersz | |
| **Połączone urządzenia** | Pozostałe urządzenia zalogowane na to samo konto | Bieżące urządzenie to „To urządzenie” |
| **Kod QR** | Kod do połączenia urządzenia | |
| **Kod numeryczny** | Kod z samych cyfr do połączenia urządzenia | |
| **Serwer** | Komputer, na którym znajduje się konto | |
| **Powiadomienia** | Ang. „notifications” | „push notifications” to „powiadomienia push” |
| **Logi** | Logi diagnostyczne | Ang. „logs” |
| **Pomoc** | Pomoc i wsparcie | |
| **Skontaktuj się z Air** | Kontakt z zespołem wsparcia | |
| **Licencje** | Informacje prawne o komponentach open source | |
| **Informacje o wersji** | Szczegóły techniczne wersji aplikacji | |
| **OK** | Ang. „Okay”, przycisk | |

## Dla tłumacza

- **Air** nie jest tłumaczone ani odmieniane
- **Nazwa użytkownika** służy do znajdowania ludzi, **nazwa wyświetlana** do rozpoznawania ich w czatach
- **Usuń** oznacza zarówno „remove”, jak i „delete”. Różnicę wskazuje dopełnienie albo „z grupy”
- **Nawiązać kontakt** i **prośba o kontakt** dotyczą ludzi, **połącz** dotyczy urządzeń. Tych pojęć nie mieszamy
- **Zablokuj** i **odblokuj** są takie same we wszystkich tekstach, także w oknach dialogowych
- Zaimki odnoszące się do użytkownika piszemy wielką literą: Ty, Cię, Twój
- Czas przeszły innej osoby ma formę z „(a)”. Czynności użytkownika mają formę bezosobową albo „przez Ciebie”
- Przypadek przejmuje rzeczownik ogólny przed symbolem zastępczym: „użytkownika {displayName}”
- Jeśli angielski tekst się zmienił, przetłumacz go od nowa zamiast poprawiać stare tłumaczenie
