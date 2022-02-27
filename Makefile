third-party:
	mkdir third-party

third-party/press-start-2p: | third-party
	mkdir third-party/press-start-2p

# Original: http://zone38.net/font
# Also seemingly available from Google Fonts, but the Google Fonts version
# appears to be modified in some unspecified manner.
third-party/press-start-2p/PressStart2P.ttf: | third-party/press-start-2p
	curl http://zone38.net/font/pressstart2p.zip > third-party/press-start-2p/pressstart2p.zip
	(cd third-party/press-start-2p && unzip pressstart2p.zip)

third-party/deja-vu: | third-party
	mkdir third-party/deja-vu

third-party/deja-vu/dejavu-fonts-ttf-2.37/ttf/DejaVuSansMono.ttf: | third-party/deja-vu
	curl -L 'https://downloads.sourceforge.net/project/dejavu/dejavu/2.37/dejavu-fonts-ttf-2.37.tar.bz2?ts=gAAAAABiHBD1yKFPYTAqgqKS_-Qsj6drqctTIjAMT_g50mYETlkiq0zT8kqLPzR_BojlVw70rpEzzVX0ITkOpzuphz7A4OTZWA%3D%3D&r=https%3A%2F%2Fsourceforge.net%2Fprojects%2Fdejavu%2Ffiles%2Fdejavu%2F2.37%2Fdejavu-fonts-ttf-2.37.tar.bz2%2Fdownload' > third-party/deja-vu/dejavu-fonts-ttf-2.37.tar.bz2
	(cd third-party/deja-vu && tar -xf dejavu-fonts-ttf-2.37.tar.bz2)
